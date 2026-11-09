# Franchise agent runtime: answers jobs for every agent whose holder registered this
# runtime's key, and serves GET /operator/<token_id> for the web app's Activate button.
FROM node:22-slim
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY tsconfig.json ./
COPY src ./src
ENV NODE_ENV=production PORT=8787
EXPOSE 8787
USER node
CMD ["npx", "tsx", "src/worker.ts"]
