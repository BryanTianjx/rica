# AUTH-08 Python Sidecar 变更说明（随打包流程落地）

- 关联方案：`docs/security/RICA-P1-多租户授权边界专项审计与整改方案-R1.0.md` §5.6 / AUTH-08
- Rust 侧已提交实现：`crates/ica-desktop/src/internal_assertion.rs`、`desktop/tauri-host/src/main.rs`、`crates/ica-contracts/src/proxy_headers.rs`
- 重要：`desktop/tauri-host/bundle-resources/**` 被 `.gitignore` 明确排除（暂存产物，不入库）。下面的 Python 改动已在本地工作区的 bundle 中打了补丁用于联调，但**必须同步到 Sidecar 的权威 Python 源码基线**，否则重新打包会丢失。

---

## 1. `app/config.py`（Settings 类，紧邻 `jwt_algorithm`）

```python
    # SEC-P1 AUTH-08: process-local secret shared with the Tauri host. It is
    # used only to verify the host's short-lived identity assertion; an empty
    # value in multiuser mode fails closed (no anonymous sidecar access).
    sidecar_assertion_secret: str = ""
```

环境变量名 `ICA_SIDECAR_ASSERTION_SECRET` 由 pydantic-settings 大小写不敏感映射而来；Tauri 主机在 `sidecar_spec()` 中注入。MultiUser 时主机同时注入 `AUTH_MODE=multiuser` 与 `JWT_SECRET`。

## 2. `app/auth/security.py`（放在 `decode_access_token` 之后）

```python
ASSERTION_ISSUER = "ica-desktop-host"
ASSERTION_AUDIENCE = "ica-sidecar"


def bearer_fingerprint(token: str) -> str:
    """SHA-256 of the bearer token, matching the Rust host's binding claim."""
    return hashlib.sha256(token.encode("utf-8")).hexdigest()


def decode_sidecar_assertion(token: str) -> dict[str, Any]:
    """Verify the host-minted identity assertion.

    Signature, audience, issuer, expiry and the required claim set are checked
    by PyJWT. The caller checks ``bearer_fp`` against the presented bearer and
    enforces ``jti`` single-use. Raises :class:`jwt.PyJWTError` on any failure.
    """
    return jwt.decode(
        token,
        settings.sidecar_assertion_secret,
        algorithms=["HS256"],
        audience=ASSERTION_AUDIENCE,
        issuer=ASSERTION_ISSUER,
        options={"require": ["exp", "iat", "sub", "jti", "iss", "aud"]},
    )
```

`hashlib`、`Any`、`jwt`、`settings` 均已在原文件导入，无需新增依赖。

## 3. `app/auth/deps.py`

导入与模块级 nonce 缓存：

```python
import time
from app.auth.security import (
    bearer_fingerprint,
    decode_access_token,
    decode_sidecar_assertion,
)

_ASSERTION_NONCE_TTL_SECONDS = 120
_seen_assertion_nonces: dict[str, float] = {}


def _consume_assertion_nonce(nonce: str, now: float) -> None:
    expired = [key for key, seen_at in _seen_assertion_nonces.items()
               if now - seen_at > _ASSERTION_NONCE_TTL_SECONDS]
    for key in expired:
        _seen_assertion_nonces.pop(key, None)
    if len(_seen_assertion_nonces) >= 4096:
        _seen_assertion_nonces.clear()
    if nonce in _seen_assertion_nonces:
        raise UnauthorizedError("Internal identity assertion was already used")
    _seen_assertion_nonces[nonce] = now


def _verify_internal_assertion(assertion: str | None, bearer: str) -> None:
    if not settings.sidecar_assertion_secret:
        raise UnauthorizedError("Sidecar identity assertion is not configured")
    if not assertion:
        raise UnauthorizedError("Missing internal identity assertion")
    try:
        claims = decode_sidecar_assertion(assertion)
    except pyjwt.PyJWTError as exc:
        raise UnauthorizedError("Invalid internal identity assertion") from exc
    if claims.get("bearer_fp") != bearer_fingerprint(bearer):
        raise UnauthorizedError("Internal identity assertion is bound to another bearer")
    _consume_assertion_nonce(str(claims.get("jti", "")), time.monotonic())
```

`get_current_user` 签名与调用的改动（其余数据库重载逻辑不变）：

```python
def get_current_user(
    authorization: str | None = Header(default=None),
    x_internal_assertion: str | None = Header(default=None, alias="x-ica-internal-assertion"),
) -> CurrentUser:
    if not settings.is_multiuser:
        return local_user()
    if not authorization or not authorization.startswith("Bearer "):
        raise UnauthorizedError("Missing bearer token")
    token = authorization[len("Bearer ") :].strip()
    # 断言只证明请求经过宿主；身份仍来自 bearer 与数据库权威记录。
    _verify_internal_assertion(x_internal_assertion, token)
    ...  # 原有 decode_access_token / DB 重载逻辑
```

断言**不**提供身份；`CurrentUser` 继续由 bearer 验签后的数据库行构造，因此禁用/降级仍即时生效。Local 模式直接返回 `local_user()`，不受影响。

## 4. 测试

随附 `app/tests/test_internal_assertion.py`（同样位于被忽略的 bundle 目录），用 `unittest` 覆盖：有效断言单次使用、伪造/过期/错受众/错签发者、bearer 绑定、未配置时失败关闭、nonce 过期。在具备 Python 的 CI 上执行：

```bash
<bundle>/python/python.exe -S -m unittest discover -s app/tests
```

本环境 `python3` 执行被阻断（退出码 49），该测试未在本轮实跑，需在 CI 补验。
