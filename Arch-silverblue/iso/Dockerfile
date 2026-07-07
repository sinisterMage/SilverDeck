# Reproducible Arch Silverblue ISO build environment.
#
# Everything needed to build the ISO lives in this image, so no Arch tooling is required on
# the host. The build itself (iso/build.sh) runs as the entrypoint; the repository is bind
# mounted at /build and the finished ISO is written to /build/iso/output/.
#
# mkarchiso needs loop devices and mount, so the container must be run with --privileged
# (see the Makefile's build-iso target).
FROM archlinux:latest

# archiso provides mkarchiso + the releng profile; the rest are used by iso/build.sh to
# assemble the overlay and the synthetic local package repository.
RUN pacman -Syu --noconfirm --needed \
        archiso \
        git \
        gptfdisk \
        dosfstools \
        libarchive \
        pacman-contrib \
    && pacman -Scc --noconfirm

WORKDIR /build
ENTRYPOINT ["/build/iso/build.sh"]
