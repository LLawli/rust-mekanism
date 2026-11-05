FROM debian:forky-slim AS launcher
ARG LAUNCHER_VERSION=0.2.42
ARG LAUNCHER_SHA256=7440d251971826ff737df61b2606f91f936d056644b3a8b6f3d5c233e7a91da9
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL -o /tmp/launcher.tar.gz \
        "https://github.com/horizon-rust/horizon/releases/download/launcher%40${LAUNCHER_VERSION}/horizon-launcher-linux-x86_64.tar.gz" \
    && echo "${LAUNCHER_SHA256}  /tmp/launcher.tar.gz" | sha256sum -c - \
    && mkdir /out && tar xzf /tmp/launcher.tar.gz -C /out

FROM debian:forky-slim
# O compilador fica na imagem final porque o launcher compila os mods nativos aqui dentro.
RUN sed -i 's/^Components: main$/Components: main contrib/' /etc/apt/sources.list.d/debian.sources \
    && apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates iptables \
        build-essential pkg-config libasound2-dev libudev-dev \
        mesa-vulkan-drivers libvulkan1 libgl1-mesa-dri libgl1 libegl1 \
        libwayland-client0 libwayland-egl1 libxkbcommon0 libxkbcommon-x11-0 libdecor-0-0 \
        libfreetype6 libfontconfig1 fonts-dejavu-core libasound2t64 libasound2-plugins libpulse0 libudev1 \
        gamescope xwayland xkb-data x11-xkb-utils libx11-6 libx11-xcb1 libxcb1 libxext6 \
        libxrandr2 libxi6 libxcursor1 libxcomposite1 libxfixes3 libxinerama1 libxrender1 \
    && rm -rf /var/lib/apt/lists/*
ENV PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/usr/games
COPY --from=launcher /out/horizon-launcher /usr/local/bin/horizon-launcher
COPY idle-watch firewall jogar /usr/local/bin/
# O jogo fala ALSA; o som sai pelo Pulse dedicado do container.
COPY asound.conf /etc/asound.conf
USER 1000:1000
CMD ["idle-watch"]
