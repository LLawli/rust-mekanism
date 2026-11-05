# O Mekanism original, para comparar comportamento. Usa a imagem do Horizon, que já tem gamescope,
# Mesa e áudio, e acrescenta o que compila e roda o mod: o JDK 25 do `java_version` e o git do grgit.
FROM jogo-horizon:latest
USER 0
RUN apt-get update && apt-get install -y --no-install-recommends openjdk-25-jdk git \
    && rm -rf /var/lib/apt/lists/*
COPY jogar /usr/local/bin/jogar
USER 1000:1000
