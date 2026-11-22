#
# Frontend image: Vite build → static files served by nginx, which also
# proxies /api and /health to the backend so the browser sees one origin
# (same shape as the dev proxy in vite.config.ts).
FROM node:20-alpine AS build

WORKDIR /app

COPY package.json package-lock.json ./
RUN npm ci

COPY . .
RUN npm run build

FROM nginx:1.27-alpine

COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /app/dist /usr/share/nginx/html

EXPOSE 80
