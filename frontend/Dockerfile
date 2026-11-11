FROM node:20-alpine as builder

WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci

COPY . .
# Optional build-time override of the source-code link (see .env.example).
# An empty value keeps the default in src/sourceCode.ts.
ARG VITE_SOURCE_CODE_URL
ENV VITE_SOURCE_CODE_URL=$VITE_SOURCE_CODE_URL
RUN npm run build

FROM nginx:alpine
COPY --from=builder /app/dist /usr/share/nginx/html

# SPA fallback — serve index.html for all routes
RUN echo 'server { listen 3001; root /usr/share/nginx/html; index index.html; location / { try_files $uri $uri/ /index.html; } }' > /etc/nginx/conf.d/default.conf

EXPOSE 3001
