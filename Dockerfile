FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
  build-essential \
  clang \
  cmake \
  curl \
  git \
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

WORKDIR /app

COPY . .

ENV DAWN_BUILD_DIR=/app/dawn-build-linux
RUN ./build.py

ENV VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json

ENTRYPOINT ["target/release/wgslsmith"]
CMD ["--help"]