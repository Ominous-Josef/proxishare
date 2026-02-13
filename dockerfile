FROM fedora:latest

# Install all necessary dependencies
RUN dnf update -y && \
    dnf install -y \
    gcc \
    gcc-c++ \
    make \
    automake \
    autoconf \
    pkgconfig \
    webkit2gtk4.1-devel \
    openssl-devel \
    curl \
    wget \
    file \
    libappindicator-gtk3-devel \
    librsvg2-devel \
    gtk3-devel \
    glib2-devel \
    cairo-devel \
    pango-devel \
    atk-devel \
    gdk-pixbuf2-devel \
    libsoup-devel \
    javascriptcoregtk4.1-devel \
    xdg-utils \
    desktop-file-utils \
    shared-mime-info \
    patchelf \
    fuse \
    fuse-libs \
    squashfs-tools \
    nodejs \
    npm && \
    dnf clean all

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Fix for AppImage building in Docker (linuxdeploy needs this to extract without FUSE)
ENV APPIMAGE_EXTRACT_AND_RUN=1

# Verify installations
RUN which xdg-open && \
    node --version && \
    npm --version && \
    rustc --version

WORKDIR /app

# Copy project files
COPY . .

# Install npm dependencies
RUN npm install

# Build the app
CMD ["npm", "run", "tauri", "build"]