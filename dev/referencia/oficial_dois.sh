#!/bin/bash
# Roda no desktop. Dois clientes do original ao mesmo tempo (decisão 14 do plano), os dois headless (um gamescope cada) e mudos (volume mestre 0 no `options.txt` de cada um):
# o ANFITRIÃO é o `runGameTestClient` de sempre, com a `CenaDois` e o roteiro `cena-dois` (ele abre o mundo à LAN na porta 25599), e o CONVIDADO é outro processo do
# mesmo cliente, lançado com a linha de comando do primeiro (o classpath do moddev só existe nela) trocando a pasta de jogo (`runs/clientB`), o nome (`Convidado`) e o
# destino (`--quickPlayMultiplayer 127.0.0.1:25599`), com o roteiro `cena-convidado`. Devolve a árvore como estava.
# Uso: oficial_dois.sh CenaDois.java cena-dois.txt SAIDA_DIR
#   `cena-dois.txt` é o roteiro do anfitrião (veja o cabeçalho da `CenaDois.java`); o convidado só obedece (um arquivo `cena-convidado` vazio basta).
#   SAIDA_DIR recebe os prints (`oficial_dois_*.png`, dos dois), `anfitriao.log` e `convidado.log` (as linhas `dois:`) e os logs do gradle.
# Pega `~/.cache/rust-mekanism/mekanism-ref.lock` com `mkdir`, espera em laço de `test -d` a cada 60 s (nunca `pgrep`) e a solta ao sair. O gradle passa pelo `lim-build`.
# Nenhum laço de espera por padrão de processo: só por arquivo e por PID.
set -u
JAVA=$(readlink -f "$1")
ROTEIRO=$(readlink -f "$2")
SAIDA=$(mkdir -p "$3" && readlink -f "$3")
DONO=${DONO:-dois-clientes}
PORTA=25599
D=$HOME/.local/share/jogo-isolado/mekanism-ref
R=$D/src/runs/client
RB=$D/src/runs/clientB
N=$D/src/src/gameTest/java/mekanism/common/tests/network
C=$HOME/.cache/rust-mekanism
LOCK=$C/mekanism-ref.lock
SYNC=$D/dois-sync
W=$C/dois-clientes
mkdir -p "$W"
# `LOCK_MEU=1`: quem chama já pegou a trava e a solta sozinho; `SEM_LIM=1`: a vaga já é de quem chama (o `lim-jogo`), e passar de novo por `lim-build` esperaria por ela para sempre.
if [ "${LOCK_MEU:-}" != 1 ]; then until mkdir "$LOCK" 2>/dev/null; do sleep 60; done; echo "$DONO" > "$LOCK/dono"; fi
LIM="$C/lim-build"; [ "${SEM_LIM:-}" = 1 ] && LIM=
echo "TRAVA $(date +%T)"
restore() {
  # Só o que é meu: o convidado e o anfitrião que subi (pelos PIDs guardados).
  for f in "$W/convidado.pid" "$W/anfitriao.pid"; do [ -s "$f" ] && kill "$(cat "$f")" 2>/dev/null; done
  rm -f "$N/CenaDois.java" "$R/cena-dois"
  [ -e "$C/cena-cabo.antes-dois" ] && mv "$C/cena-cabo.antes-dois" "$R/cena-cabo"
  [ -e "$C/options.txt.antes-dois" ] && cp "$C/options.txt.antes-dois" "$R/options.txt" && rm -f "$C/options.txt.antes-dois"
  for p in $(docker top mekanism-ref -o pid,args 2>/dev/null | grep -E "devlaunch.[M]ain|gradle-wrapper|gamescope|[X]wayland" | awk '{print $1}'); do kill "$p" 2>/dev/null; done
  sleep 5
  for p in $(docker top mekanism-ref -o pid,args 2>/dev/null | grep -E "devlaunch.[M]ain|gradle-wrapper|gamescope|[X]wayland|GradleDaemon|gradle.*launcher" | awk '{print $1}'); do kill -9 "$p" 2>/dev/null; done
  rm -rf "$RB" "$SYNC" "$D/dois-guest"
  [ "${LOCK_MEU:-}" = 1 ] || rm -rf "$LOCK"
  echo "RESTAURADO $(date +%T)"
}
trap restore EXIT
ocupado() { docker ps --format '{{.Names}}' | grep -qx mekanism-ref && docker top mekanism-ref -o args | tail -n +2 | grep -vE 'docker-init|idle-watch|^sleep ' | grep -q .; }
for i in $(seq 30); do ocupado || break; sleep 20; done
if ocupado; then echo OCUPADO; exit 3; fi
cp "$R/options.txt" "$C/options.txt.antes-dois"
sed -i 's/^soundCategory_master:.*/soundCategory_master:0.0/' "$R/options.txt"
[ -e "$R/cena-cabo" ] && mv "$R/cena-cabo" "$C/cena-cabo.antes-dois"
rm -rf "$RB" "$SYNC"
# O log do jogo anterior fica em `latest.log` e tem as linhas `dois:` dele: some antes, para a espera da LAN não achar a de outra rodada.
rm -f "$R/logs/latest.log"
mkdir -p "$RB"
cp "$R/options.txt" "$RB/options.txt"
: > "$RB/cena-convidado"
cp "$JAVA" "$N/CenaDois.java"
cp "$ROTEIRO" "$R/cena-dois"
JANELA="-W 1920 -H 1080 -w 1920 -h 1080"
# O anfitrião: o gradle compila, abre o mundo e roda a cena. O PID guardado é o do `lim-build` (o `exec` o troca pelo `flock` e depois pelo `docker exec`).
( $LIM env JOGO_CMD='gamescope --backend headless '"$JANELA"' -r 15 -- env -u WAYLAND_DISPLAY sh -c "cd \"\$FONTE\" && ./gradlew --no-daemon --max-workers=4 runGameTestClient"' "$HOME/.local/bin/mekanism-ref" > "$SAIDA/gradle_anfitriao.log" 2>&1 & echo $! > "$W/anfitriao.pid"; wait )  &
ESPERA=$!
# Espera a LAN abrir (o log do anfitrião), no máximo 25 minutos, de 10 em 10 s, por arquivo.
for i in $(seq 150); do
  grep -qs "dois: lan $PORTA true" "$R/logs/latest.log" && break
  kill -0 "$ESPERA" 2>/dev/null || { echo "o anfitrião acabou antes da LAN"; break; }
  sleep 10
done
if grep -qs "dois: lan $PORTA true" "$R/logs/latest.log"; then
  # A linha de comando do processo do cliente do anfitrião (`devlaunch.Main` com os argumentos num arquivo `@...RunProgramArgs.txt`; `docker top` dá o PID do host), para o
  # convidado: o mesmo processo com uma cópia do arquivo de argumentos que entra na LAN com outro nome. O container só enxerga a pasta de dados do `mekanism-ref`, então o
  # lançador e a cópia ficam em `$G`.
  G=$D/dois-guest
  mkdir -p "$G"
  PID=$(docker top mekanism-ref -o pid,args | grep "devlaunch.[M]ain" | grep "gameTestClientRun" | awk '{print $1}' | head -1)
  echo "cliente do anfitrião: $PID"
  cp "/proc/$PID/cmdline" "$W/anfitriao.cmdline"
  python3 -I - "$W/anfitriao.cmdline" "$PORTA" "$G/convidado.sh" "$G/convidado_args.txt" "$RB" <<'PY'
import shlex, sys
raw, porta, saida, argfile, rb = sys.argv[1:]
args = [a.decode() for a in open(raw, "rb").read().split(b"\0") if a != b""]
out = []
for a in args:
    if a.startswith("@") and a.endswith("gameTestClientRunProgramArgs.txt"):
        linhas = open(a[1:], encoding="utf-8").read().split("\n")
        novas, i = [], 0
        while i < len(linhas):
            if linhas[i].strip() == "--quickPlaySingleplayer":
                novas += ["--quickPlayMultiplayer", f"127.0.0.1:{porta}"]
                i += 2
                continue
            novas.append(linhas[i])
            i += 1
        novas += ["", "--username", "Convidado"]
        open(argfile, "w", encoding="utf-8").write("\n".join(novas) + "\n")
        out.append("@" + argfile)
    else:
        out.append(a)
open(saida, "w").write(f"cd {shlex.quote(rb)} && exec " + " ".join(shlex.quote(a) for a in out) + "\n")
PY
  ( JOGO_CMD='gamescope --backend headless '"$JANELA"' -r 15 -- env -u WAYLAND_DISPLAY sh '"$G/convidado.sh" "$HOME/.local/bin/mekanism-ref" > "$SAIDA/gradle_convidado.log" 2>&1 & echo $! > "$W/convidado.pid"; wait ) &
fi
# O anfitrião fecha o jogo no `fim` do roteiro (ou o `restore` o mata): espera por PID, no máximo 40 minutos.
for i in $(seq 240); do kill -0 "$ESPERA" 2>/dev/null || break; sleep 10; done
echo "FIM $(date +%T)"
cp "$R"/screenshots/oficial_dois_*.png "$SAIDA/" 2>/dev/null; rm -f "$R"/screenshots/oficial_dois_*.png
for f in "$RB"/screenshots/oficial_dois_*.png; do [ -e "$f" ] && cp "$f" "$SAIDA/convidado_$(basename "$f")"; done
grep -a "dois:" "$R/logs/latest.log" > "$SAIDA/anfitriao.log" 2>/dev/null
grep -a "dois:" "$RB/logs/latest.log" > "$SAIDA/convidado.log" 2>/dev/null
cp "$D/dois-sync/respostas.jsonl" "$SAIDA/respostas.jsonl" 2>/dev/null
