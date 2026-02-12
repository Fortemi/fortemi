# Lightweight 3D model renderer using Three.js
# Much smaller than Blender (~100MB vs 2GB)
FROM node:20-slim

# Install dependencies for headless-gl
RUN apt-get update && apt-get install -y --no-install-recommends \
    libgl1-mesa-dev \
    libxi-dev \
    libxext-dev \
    xvfb \
    python3 \
    make \
    g++ \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy package files first for layer caching
COPY package.json ./

# Install dependencies (including native gl module)
RUN npm install --production

# Copy server code
COPY server.js ./

# Also install pngjs and canvas for image processing
RUN npm install pngjs canvas --production

ENV PORT=8080
EXPOSE 8080

# Run with xvfb for virtual framebuffer
CMD ["xvfb-run", "-a", "--server-args=-screen 0 1024x768x24", "node", "server.js"]
