# ICA-RUST W4 OCR 独立审计报告

审计对象：`w4-candidate` = `304d2da`（快照 `/tmp/audit-w4-2`，非 git 仓，只读）
基线：`/mnt/o/ica` @ `9ed173c`（工作树干净，0 个未提交改动）
审计时间：2026-11-19 23:20–00:20 CST
审计方式：**全部结论来自本次实测**；无一处引用实现者说法。凡属读码推断或无法构造的路径，均在文中显式标注。

---

## 0. 结论摘要

**VERDICT: FAIL**

「识别文本与 Python 基线逐字符一致」这一条**不成立**。同一张图、同一套模型，Rust 返回的 `text`
与基线在**行切分**上不同：基线把 `Candidate: hi there` 切成 `"hi"` / `"there"` 两个框，Rust 合成一个框，
于是 `\n` 与空格互换（第 23 个字符 `U+000A` vs `U+0020`，总长都是 29）。这不是偶发，**三次复跑完全一致**；
也不是随机噪声，而是由 `rapidocr-core` 的**检测器输入上限**（4096 边长 / 2048² 像素 / `Downscale`）
与基线「无上限」造成的分辨率差：实测在图像宽度 **757 px** 处翻面（740 一致、760 起分叉），
与算式 `4096×136/736 = 757` 逐像素吻合。

字符过滤、路径守卫、错误码、暂存文件清理这四块**实测等价**，没有问题。
另发现 3 处次要差异（`details.reason` 文案、503 message 常量、推理失败的状态码 409 vs 500）
和 2 处健壮性缺陷（引擎失败终态被永久缓存、锁中毒语义）。

| 项 | 判定 | 依据 |
| --- | --- | --- |
| A 字符过滤集合与后处理流程 | **PASS**（集合正确、流程等价） | 实测集合 6 成员含 U+002D；逐行对照见 §2 |
| B 端到端逐字符一致 | **FAIL** | 8 张图 + 9 档宽度 + 6 档字号实测，1 类布局稳定分叉 |
| C 路径守卫 / 错误码 / 残留 | **PASS**（1 处 details 文案差异） | 13 条用例状态码与 code 全同，残留 0 |
| D 资源与健壮性 | **PASS with findings** | 串行化正确、无 fd 泄漏、RSS 平台期；2 处缺陷 |

---

## 1. 环境与基线版本（实测）

```
$ git -C /mnt/o/ica rev-parse --short HEAD
9ed173c
$ git -C /mnt/o/ica status --porcelain -uall | wc -l
0                     # 基线工作树干净

$ git -C /mnt/o/ica diff --stat d39e321..HEAD -- backend/app/application/ocr_service.py \
      backend/app/api/imports.py backend/app/schemas/conversation.py
 backend/app/api/imports.py | 32 +++++++++++++++++++++++++++++++-
 1 file changed, 31 insertions(+), 1 deletion(-)
```

`imports.py` 的 31 行增量**只有**新增的 `POST /imports/{id}/confirm-candidate` 路由（diff 已核），
**未触及 OCR 段**；`ocr_service.py` 与 `conversation.py` 零 diff。→ 本报告的 OCR 结论对 `d39e321` 与 `9ed173c`
同时成立，不存在「基线漂移导致结论作废」。

两侧服务均在独立沙箱内启动，基线以副本运行（`/tmp/w4audit/pybase`，rsync 排除 `.venv/.env/*.db`），
`/mnt/o/ica` 全程零写入（`git status` 0 项，`ica.db` sha256 `466aa3fccf96e98d…` 未变）。

```
py: ICA_DATA_DIR=/tmp/w4audit/py-data  DATABASE_URL=sqlite:////tmp/w4audit/py.db   port 8020
rs: ICA_DATA_DIR=/tmp/w4audit/rs-data  DATABASE_URL=sqlite:////tmp/w4audit/rs.db   port 8021
    RAPIDOCR_MODEL_DIR=<venv>/rapidocr/models
两侧独立 SQLite（各 31 张表，由同一 `Base.metadata.create_all` 生成后复制）
```

模型同一性由我自己核对（不引用文档）：

```
$ sha256sum <venv>/rapidocr/models/*
090f04ab… PP-OCRv6_det_small.onnx      6f327246… PP-OCRv6_rec_small.onnx
e47acedf… ch_ppocr_mobile_v2.0_cls…    b5f2bfe2… ppocrv6_dict.txt
```
四个哈希与 `rapidocr-core-0.3.0/src/model.rs` 里 `PPOCRV6_DET/REC/DICT/CLS` 的期望值逐个相同；
`det` 参数也一致（`limit_side_len 736`、`limit_type Min`、`thresh 0.3`、`box_thresh 0.5`、
`unclip_ratio 1.6`、`text_score 0.5`）。**差异不在模型、也不在阈值。**

---

## 2. A 部分：字符过滤集合与后处理流程

### 2.1 集合真实成员（我自己 print 的）

```
$ python - <<'EOF'
s = set('\ufffd\x00-\x08\x0e-\x1f')
print("len =", len(s))
...
EOF
len = 6
  U+0000  '\x00'
  U+0008  '\x08'
  U+000E  '\x0e'
  U+001F  '\x1f'
  U+002D  '-'  HYPHEN-MINUS
  U+FFFD  '�'  REPLACEMENT CHARACTER
hyphen in set: True
U+0009 tab in set: False      U+000A LF in set: False      U+001C in set: False
```

**确认**：这是集合字面量（`-` 出现两次被去重），成员恰好是上述 6 个，**含 U+002D**。
所以 `2026-11-18` → `20261118` 是基线行为，不是实现者的误读。

### 2.2 两侧过滤与后处理逐行对照

| 步骤 | 基线 `read_ocr_text`（`ocr_service.py:125-135`） | Rust `read_text`/`run_inference`/`clean`（`ocr_rapid.rs:196-233,57-63`） | 等价？ |
| --- | --- | --- | --- |
| 组装行 | `"\n".join(result.txts or ())` | `lines.iter().map(|l| l.text).collect::<Vec<_>>().join("\n")` | ✅（空结果都是 `""`） |
| 行顺序 | RapidOCR 两步排序（先 y 后 x，带行阈值） | 同（`db_postprocess.rs:496 sort_candidates` 注释明写 match RapidOCR） | ✅ 读码 |
| 低分过滤 | `Global.text_score: 0.5` 丢弃低分框 | `cfg.text_score` 默认 0.5（`lib.rs:278`） | ✅ 读码 |
| 字符过滤 | `for ch in text: if ch in _EXCLUDED_CHARS: continue` | `chars().filter(|c| !EXCLUDED_CHARS.contains(c))` | ✅ |
| 集合内容 | 6 个（见上） | `['\u{0}','\u{8}','\u{e}','\u{1f}','\u{2d}','\u{fffd}']` = 同 6 个 | ✅ |
| 首尾裁剪 | `.strip()`（Python `str.isspace()`：含 U+001C–U+001F） | `.trim()`（`char::is_whitespace`：Unicode White_Space，**不含** U+001C–U+001E） | ⚠️ 读码差异，实测不可达（U+001F 已在过滤集内，U+001C–1E 不会出现在 OCR 输出） |
| 空行处理 | 不删空行 | 不删空行（`join` 保留 `""` 行） | ✅ 实测：`blank` 图两侧都 `text=''` |
| 额外 trim/去重/排序 | 无 | 无 | ✅ |

**A 结论**：过滤逻辑与后处理链**等价**，实现者对集合的理解**正确**。
Rust 侧唯一的额外动作就是 `trim`，与基线 `strip` 同位置、同语义（除上述不可达的 3 个控制字符）。

---

## 3. B 部分：端到端逐字符对比（核心）

### 3.1 逐字符对比表（4 张主图，每侧独立暂存副本）

`repr()` 对比，逐码位打印。探针：`/tmp/w4audit/parity.py`，原始 JSON `/tmp/w4audit/parity.json`。

| # | 图（尺寸/内容） | python `repr` | rust `repr` | 结果 |
| --- | --- | --- | --- | --- |
| 1 | `english` 1000×136<br>Hello OCR / Candidate: hi there | `'Hello OCR\nCandidate: hi\nthere'` | `'Hello OCR\nCandidate: hi there'` | **DIFF @char 23**：py `U+000A` vs rs `U+0020`（两边均 29 字符） |
| 2 | `dates` 1000×184<br>2026-11-18 10:00 / Q1-2026 review 03-04-2026 | `'2026111810:00\nRecruiter: welcome\nQ12026 review 03042026'` | 同 | **IDENTICAL 55 字符**（连字符全部被删，两侧一致） |
| 3 | `mixed` 1000×184<br>候选人：张伟 / Email: zhang@example.com / 期望薪资: 25K-30K | `'候选人：张伟\nEmail: zhang@example.com\n期望薪资：25K30K'` | 同 | **IDENTICAL 43 字符** |
| 4 | `special` 1000×184<br>Rate: $120/hour #1 100% / Contact: @alice_88 (WeChat) / 长英文行 | `'Rate: $120/hour #1 100%\nContact: @alice_88 (WeChat)\nCandidate: 8 years …distributed sys'` | 同 | **IDENTICAL 117 字符**（第 3 行被画布右缘截断，两侧同样截断；未截断的长行由 §3.2 `longline` 覆盖） |

补充 4 例（`/tmp/w4audit/extra.py`）：

| # | 图 | python | rust | 结果 |
| --- | --- | --- | --- | --- |
| 5 | `blank` 800×300 纯白 | `''` | `''` | IDENTICAL（都 200，session 形状同） |
| 6 | `longline` 1500×200 超长英文行 | `'Candidate: 8 years … in Go and Rust'`（77 字符，完整） | 同 | **IDENTICAL** |
| 7 | `twocol` 900×260 双栏 | `'Candidate: Alice\n20261118\nRecruiter: Bob\nSalary: 30K'` | 同 | **IDENTICAL**（栏间阅读顺序一致） |
| 8 | `phone` 1170×900 手机截图体（含日期、电话） | `'Recruiter\n…\nMy number is 13800138000.'` | 同 | **IDENTICAL**（59 字符） |

两侧响应 key 集合与 `language` 全部一致：`['candidate_id','created_at','language','session_id','text']` / `zh+en`。

### 3.2 差异是可复现且可定界的（三次复跑 + 两维扫描）

同一张 `english.png` 连打 3 次：

```
run0 py='Hello OCR\nCandidate: hi\nthere'   rs='Hello OCR\nCandidate: hi there'  equal=False
run1 py='Hello OCR\nCandidate: hi\nthere'   rs='Hello OCR\nCandidate: hi there'  equal=False
run2 py='Hello OCR\nCandidate: hi\nthere'   rs='Hello OCR\nCandidate: hi there'  equal=False
```

**宽度扫描**（同一两行文本、字号 30、高度固定 136），假设是检测器输入上限在 `736/136×w > 4096` 处生效：

```
clamp threshold width = 757 px (736/136 * w > 4096)
  w= 520  clamp=False  equal=True    py='Hello OCR\nCandidate: hi there'
  w= 600  clamp=False  equal=True    py='Hello OCR\nCandidate: hi there'
  w= 700  clamp=False  equal=True    py='Hello OCR\nCandidate: hi\nthere'
  w= 740  clamp=False  equal=True    py='Hello OCR\nCandidate: hi\nthere'
  w= 760  clamp=True   equal=False   py='Hello OCR\nCandidate: hi\nthere'  rs='… hi there'
  w= 800  clamp=True   equal=False   同上
  w= 900  clamp=True   equal=False   同上
  w=1000  clamp=True   equal=False   同上
  w=1200  clamp=True   equal=True    两侧都合框（巧合，不改变机制）
```

翻面点落在 740 与 760 之间 —— 与 `4096×136/736 = 757` 吻合到像素级。

**字号扫描**（宽度 1000）：

```
size=22 equal=True    size=26 equal=True    size=30 equal=False
size=34 equal=False   size=40 equal=False   size=46 equal=True（两侧都切成 4 段）
```

### 3.3 根因（代码 + 实测双向确认）

`rapidocr-core` 的 `ModelSetSpec::config()`（`model.rs:343-360`）在构造 `DetConfig` 之后**显式**写：

```rust
DetConfig::new(…)
    .with_input_limits(DetInputLimits::default())     // model.rs:359
```

而 `DetInputLimits::default()`（`config.rs:757-764`）是：

```rust
max_side_len: Some(4096),
max_pixels: Some(2048 * 2048),      // 4,194,304
overflow_behavior: Downscale,       // 超限就等比缩小
```

基线 Python（`rapidocr/config.yaml`）**没有任何输入上限**：`Det.limit_side_len: 736 / limit_type: min`
原样上采样到 `736/136×1000 ≈ 5412 × 736`（8.0 MPx），Rust 被压到 `4096 × 557`（2.3 MPx）。
检测框在两张不同分辨率的特征图上生成，于是一行 `hi there` 在基线里裂成两框、在 Rust 里合为一框。

**这不是模型差异，也不是阈值差异**：宽度 ≤ 740 px（4 档全部不触发上限）时**逐字符完全相同**，
字号 22/26 两档也完全相同；只有触发上限的档位才分叉。实现者只传了
`with_pipeline(PipelineConfig::full())`，没有覆盖 `input_limits`。

### 3.4 下游影响（实测）

差异字符会被写进 `import_records.content`（`preview_import` 存 `raw_text`）：

```
$ python -c "from app.application.conversation_service import parse_messages; …"
python-side -> 1 message(s), 2 warning(s)   content='Hello OCR\nCandidate: hi\nthere'
rust-side   -> 1 message(s), 2 warning(s)   content='Hello OCR\nCandidate: hi there'
```

本图恰好仍是 1 条消息，但**落库内容不同**；对「前缀识别发送人」的文本（`Recruiter:` / `候选人：`），
断行位置直接决定后续行是否被并入同一条消息。这正是契约文档自己强调的
「文本要喂给 import 解析器，差一个换行就可能改变解析结果」。

### 3.5 建议修复（一行）

```rust
let config = cache.config_for(model_set)
    .with_pipeline(PipelineConfig::full());
let mut det = config.det.clone();          // 或等价地直接改字段
det.input_limits = DetInputLimits::unrestricted();   // 与基线一样不设上限
```
回归要求：至少 3 张 **≥ 1000 px 宽**的真实截图（而非 760 px 以下的小图），
否则这个缺陷在测试里永远看不见 —— 实现者的 §13.4「三张图全部一致」正是被这个尺寸盲区掩盖的。

---

## 4. C 部分：路径守卫、错误码、清理

探针 `/tmp/w4audit/c_probe.py`，完整输出 `/tmp/w4audit/c_probe.log`。两侧同时打同一类输入，
错开文件名（暂存文件会被删除，这是探针陷阱不是实现差异）。

| # | 输入 | python | rust | 判定 |
| --- | --- | --- | --- | --- |
| C1 | 暂存目录外的绝对路径 | 409 `ocr_path_not_allowed`，msg 回显原始路径，`details.allowed_prefix` | 同 | ✅ SAME |
| C2 | `../../etc/passwd` | 409 `ocr_path_not_allowed` | 同 | ✅ SAME |
| C3 | 传目录（暂存根） | 409 `ocr_path_not_allowed`「must be a file, not a directory」 | 同 | ✅ SAME |
| C4 | 传目录（暂存子目录） | 409 同上 | 同 | ✅ SAME |
| C5 | 存在性：暂存内不存在的文件 | 409 `business_rule_violation`「OCR image not found: …」`details={}` | 同 | ✅ SAME（`ocr_image_missing` 确为死代码，两侧都没走那条） |
| C6 | 非图片文件（文本存成 .png） | 409 `ocr_image_undecodable`，reason=`cannot identify image file '…'` | 409 同 code，reason=`The image format could not be determined` | ⚠️ code/status 同，**details.reason 文案不同** |
| C7 | 0 字节文件 | 409 `ocr_image_undecodable` | 同 code | ⚠️ 同上 |
| C8 | 截断 PNG（前 120 字节） | 409 `ocr_image_undecodable`，reason=`image file is truncated` | 同 code，reason=`unexpected end of file` | ⚠️ 同上 |
| C9 | 暂存内子目录中的图片 | 200（允许，`relative_to` 命中） | 200 | ✅ SAME |
| C10 | 暂存内符号链接 → 目录外 | 409 `ocr_path_not_allowed`，文件未删 | 同 | ✅ SAME（守卫在删除之前，语义正确） |
| C11 | 暂存内符号链接 → 暂存内 | 200，目标被删 | 200 | ✅ SAME（都留下一个悬空链接，见下） |
| C12 | `candidate_id: null` | 200（OCR 成功） | 200 | ✅ SAME |
| C13 | `candidate_id` 不存在 + OCR 成功 | 404 `not_found`「Candidate '…' was not found」 | 同 | ✅ SAME，且**文件已删**（两侧 `file_left=False`） |

**顺序正确性**：C1/C2 的 message 回显**原始输入路径**而不是解析后的路径，也不泄露文件是否存在
（越界路径不论存不存在都是 `ocr_path_not_allowed`）；C5 才暴露存在性 —— 与基线
「先路径守卫、后存在性」的书写顺序一致。C3（目录）先于越界判定，与基线同。

**暂存文件清理（逐路径实测）**：

| 路径 | py 残留 | rs 残留 |
| --- | --- | --- |
| 成功 200（C9/C11/C12，含 §3 全部图） | 无 | 无 |
| 解码失败 409（C6/C7/C8） | 无 | 无 |
| preview 404（C13） | 无 | 无 |
| 引擎不可用 503（§5.2 专用实例） | 无 | 无 |
| 路径越界 409（C1/C2/C10） | **保留**（正确：守卫不删） | 同 |
| 符号链接→暂存内（C11） | 目标删掉、**留下悬空链接** | 同（与基线同构） |

全流程结束后暂存目录只剩 C10/C11 我自己造的探针产物（`link-*.png` / `linkin-*.png` / `sub/`），
没有任何一张截图残留。

---

## 5. D 部分：资源与健壮性

### 5.1 并发：确实串行化（实测）

`engine()` 返回 `&'static Arc<Mutex<RapidOcr>>`，`run_inference` 全程持锁；推理在 `spawn_blocking` 里跑。

```
### 4 并发打到 Rust（4 张不同图）
  wall=23.13s  per-request=55.24s   statuses=[200,200,200,200]
### 4 并发打到 Python（对照）
  wall=12.98s  statuses=[200,200,200,200]
### 单发（4 张图顺序）
  rs times=[4.12, 5.57, 5.37, 5.71]   py times=[4.62, 3.10, 3.14, 3.84]
```

4 并发 23.1 s ≈ 4 × 5.5 s → **完全串行**，与代码一致（`RapidOcr::run_path` 需要 `&mut self`，无法并行）。
基线是无锁并发（`_ENGINE_LOCK` 只护初始化），因此同样 4 个请求 Python 13 s、Rust 23 s。
这是**已知且合理**的取舍（doc comment 有说明），非缺陷；但意味着桌面端「拖 5 张截图」会排队约 25 s，
产品上要能接受。附带风险（读码）：每个请求占一个 blocking 线程等锁，极端并发下会吃满 512 线程池。

### 5.2 模型缺失：可读 503，不 panic，文件仍被删（实测）

用 `RAPIDOCR_MODEL_DIR=/tmp/w4audit/nomodels`（空目录）另起 8022：

```
HTTP 503
{"error":{"code":"ocr_unavailable",
 "message":"Local OCR is not available on this machine. Install the optional OCR stack to use screenshot import.",
 "details":{"reason":"OCR models are not available: failed to prepare PP-OCRv6 small detection model:
   missing model asset … at /tmp/w4audit/nomodels/PP-OCRv6_det_small.onnx; allow downloads or place the file in the model cache"}}}
$ ls -l /tmp/w4audit/nomodel-data/ocr/ ; total 0        # 暂存文件已删
```

启动日志无 panic，`ModelDownloadMode::Never` 生效（不联网、不卡死）。

### 5.3 内存与句柄（实测）

```
### 顺序 4 发
  py rss 256060→265276 kB (+9216)   fds 17→17
  rs rss 265632→426072 kB (+160440) fds 15→15
### 再连打 12 发（Rust）
  start 429504 → 650996 → 702748 → … → 702852 kB（第 2 发起稳定在 ~703 MB）
### 24 MPx 大图（6000×4000）
  py 200 3.6s rss_delta +278 MB text=''
  rs 200 5.2s rss_delta +0       text=''
```

结论：**没有无界泄漏**（12 发后 RSS 平台化在 ~703 MB），**文件句柄零增长**（`/proc/<pid>/fd` 恒定 15）。
但绝对值偏高：从 265 MB 涨到 ~703 MB（ONNX arena + 图像缓冲），是 Python 基线的 ~2.7 倍；
24 MPx 图两侧都返回**空文本**（`limit_type=min` 把 6000×4000 缩到 1104×736，字太小检不出）——行为一致，但值得知道。

### 5.4 锁中毒与失败缓存（读码判断，未能实测）

- `run_inference` 的 `engine.lock()` 失败 → `"OCR engine lock was poisoned"` → 落到
  `ApplicationError::Unavailable` → **500 `internal_error`**（不是 `ocr_error`）。
  只会在持锁期间 panic（ONNX 内部 panic）时触发；一旦中毒，**后续所有请求永久 500**。
- `OnceLock<Result<Arc<Mutex<RapidOcr>>, String>>` 把**失败也缓存成终态**：第一次请求时模型目录没就绪，
  之后即使把模型放好，该进程仍然一直 503，必须重启进程。（读码判断：`get_or_init` 的闭包只执行一次；
  我只发了 1 次请求验证 503 可读，未做「补上模型后再请求」的实测。）

---

## 6. 缺陷清单（按严重度）

| # | 级别 | 缺陷 | 证据 | 性质 |
| --- | --- | --- | --- | --- |
| **F1** | **P1 阻断** | 检测器输入被 `DetInputLimits::default()` 钳制（4096 / 2048² / Downscale），Python 无上限 → 行切分不同 → `text` 出现 `\n`↔空格 差异，落库 `import_records.content` 随之为另一份文本 | §3.1–3.4；757 px 翻面点 | **实测确认** |
| **F2** | P3 低 | 推理失败映射 `409 ocr_error`；基线 `raise ICAError(..., code="ocr_error")` 的 `status_code` 默认是 **500**（`errors.py:17`，`main.py:127-129` 直接回 `exc.status_code`） | 读码：`ocr_service.py:123` + `errors.py:14-18`；**未能构造**可解码但推理失败的图 | 读码判断（未实测） |
| **F3** | P3 低 | `ocr_image_undecodable` 的 `details.reason` 文案不同（Pillow `cannot identify image file '…'` vs image crate `The image format could not be determined`；截断图 `image file is truncated` vs `unexpected end of file`） | §4 C6/C7/C8 | 实测确认 |
| **F4** | P3 低 | 503 的 `message` 与基线常量不同：基线是 `Local OCR is not available on this machine. Install the optional OCR stack with \`uv pip install -e '.[ocr]'\` (RapidOCR + ONNX Runtime) to use screenshot import.`，Rust 少了中间半句 | 读码 `ocr_service.py:88-96` vs `ocr_rapid.rs:141-143`；本机基线 `OCR_AVAILABLE=True`，**该分支不可达，未能实测** | 读码判断（未实测） |
| **F5** | P2 中 | 引擎初始化失败被 `OnceLock` 永久缓存 → 装好模型也要重启进程 | §5.4 | 读码判断 |
| **F6** | P2 中 | 锁中毒后永久 500 且 code 是 `internal_error`（语义应为 `ocr_error`） | §5.4 | 读码判断 |
| F7 | 观察 | `strip` vs `trim` 的空白字符集差异（U+001C–U+001E） | §2.2 | 读码判断，实测不可达 |

> F2 的严重度取决于「能否让引擎在解码之后失败」。我尝试的路径（非图片、0 字节、截断、24 MPx、空图）
> 全部在解码阶段就被拦下，**未能实测 409 vs 500**。按读码它是一处真实分叉，建议实现方补一条
> 「解码成功但推理抛错」的注入测试后定级。

---

## 7. 实测确认 / 读码判断 / 未能验证

**实测确认（本报告每条都有命令输出）**
1. `_EXCLUDED_CHARS` 恰为 6 个成员且含 U+002D；Rust 侧同 6 个。
2. 4 张主图 + 4 张补充图逐字符对比；`english` 图稳定分叉（3/3 复跑），其余 7 张全同。
3. 分叉边界 = 图像宽度 757 px（宽度扫描 9 档、字号扫描 6 档）。
4. 模型 4 个文件 sha256 与 crate 期望值一致；det/rec/cls/text_score 参数一致。
5. 路径守卫 13 条用例状态码+code 全同；越界不泄露存在性；目录先判。
6. 暂存文件在 200 / 409(undecodable) / 404(preview) / 503(无模型) 四类路径都被删除。
7. 4 并发串行化（23.1 s），Python 13.0 s。
8. RSS 平台化 ~703 MB、fd 零增长；24 MPx 大图两侧同返空文本。
9. 模型缺失 → 503 可读原因、无 panic、文件已删。

**读码判断（未构造出触发条件）**
- F2（409 vs 500）、F4（503 message 常量）、F5（失败终态缓存）、F6（锁中毒）、F7（strip/trim 字符集）。

**未能验证**
- 真实手机截图（含状态栏、抗锯齿、JPEG 噪声）的语料级抽样：本次只用 PIL 合成图。
  **F1 的存在说明这类语料很可能有更高比例的分叉**，但比例未测。
- 多用户（`is_multiuser=True`）下的暂存分区语义：只测了 local 模式扁平目录。
- 并发下 Python 基线是否真的线程安全（本次 4 并发未见异常，但未做压力验证）。

---

## 8. 复跑命令

```bash
# 环境（本报告所用）
/tmp/w4audit/mkdb.py          # 生成 31 张表的空库，复制成 py.db / rs.db
/tmp/w4audit/mkimg.py         # 4 张主图
/tmp/w4audit/extra.py         # blank / longline / twocol / phone
# 启动（两端口、两 data dir、两库）
env -u PYTHONPATH -u PYTHONHOME ICA_DATA_DIR=/tmp/w4audit/py-data \
  DATABASE_URL=sqlite:////tmp/w4audit/py.db AI_API_KEY= \
  /mnt/o/ica/backend/.venv/bin/python -m uvicorn app.main:app --port 8020   # cwd=pybase
ICA_DATA_DIR=/tmp/w4audit/rs-data DATABASE_URL=sqlite:////tmp/w4audit/rs.db PORT=8021 \
  RAPIDOCR_MODEL_DIR=/mnt/o/ica/backend/.venv/lib/python3.12/site-packages/rapidocr/models \
  /home/bryan/.cache/ica-rust-target/release/ica-server
# 探针
python /tmp/w4audit/parity.py     # §3.1 逐字符表
python /tmp/w4audit/repeat.py     # §3.2 复跑 + 字号扫描
python /tmp/w4audit/widths.py     # §3.2 宽度扫描（757 px 翻面点）
python /tmp/w4audit/c_probe.py    # §4 守卫/错误码/残留
python /tmp/w4audit/d_probe.py    # §5.1 并发 + RSS/fd
python /tmp/w4audit/d5_probe.py   # §5.3 内存平台 + 大图
```

**清理状态**：8020/8021/8022 三个进程已 kill，`ss -ltn` 三端口均为空；
基线 `/mnt/o/ica` 零改动；审计快照 `/tmp/audit-w4-2` 除本报告外未写入；
`/mnt/o/ICA-RUST` 有 1 项未跟踪文件 `docs/ICA Browser Subsystem Architecture R1.0.md`，**不是本审计产生**（我全程未写该仓）。

---

## VERDICT: **FAIL**

理由不是「细节不完全一致」，而是**声称的那条性质本身不成立**：在 ≥760 px 宽的截图上，
Rust 端口输出的换行位置会与基线不同，且该文本直接进入 `import_records.content`。
修复面很小（`DetInputLimits::unrestricted()` 或等价策略），但**验收必须换成 ≥1000 px 宽的多行真实截图**，
否则同样的缺陷会再次从 760 px 以下的测试图里漏过去。
其余（字符过滤集合、路径守卫顺序与状态码、暂存清理、串行化与资源）实测与基线一致，无需返工。
