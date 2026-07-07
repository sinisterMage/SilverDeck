# Arch build container for the SilverDeck pacman packages.
# makepkg refuses to run as root, hence the dedicated builder user.
FROM archlinux:latest

RUN pacman -Syu --noconfirm --needed \
        base-devel git rust pkgconf cmake clang \
        wayland libxkbcommon libxkbcommon-x11 libxcb vulkan-headers vulkan-icd-loader \
        fontconfig freetype2 alsa-lib systemd-libs \
    && pacman -Scc --noconfirm

RUN useradd -m builder
USER builder
# Pre-create the cargo home so the named cache volume mounted here inherits
# builder ownership instead of root's.
RUN mkdir -p /home/builder/.cargo
WORKDIR /work
