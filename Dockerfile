FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
  build-essential \
  clang \
  cmake \
  curl \
  default-jre-headless \
  git \
  nano \
  python3 \
  ninja-build \
  pkg-config \
  libclang-dev \
  libx11-dev \
  libxi-dev \
  libxcursor-dev \
  libxrandr-dev \
  libwayland-dev \
  vulkan-tools \
  libvulkan-dev \
  mesa-vulkan-drivers \
  && rm -rf /var/lib/apt/lists/*

RUN git clone https://chromium.googlesource.com/chromium/tools/depot_tools.git /opt/depot_tools
ENV PATH="/opt/depot_tools:${PATH}"

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

RUN curl -L -o /opt/perses_deploy.jar https://github.com/mandryskowski/perses/releases/latest/download/perses_deploy.jar

RUN mkdir -p ~/.config/wgslsmith && \
    echo "[reducer.perses]\njar = \"/opt/perses_deploy.jar\"" > ~/.config/wgslsmith/wgslsmith.toml

WORKDIR /app

COPY . .

ENV DAWN_BUILD_DIR=/app/dawn-build-linux
RUN ./build.py

ENV VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json

CMD ["/bin/bash"]