#!/usr/bin/env python3
"""Grava o áudio do port (`horizon`) pelo sink nulo, nunca pelas caixas do usuário, enquanto `h2_som_medida.py` roda os cenários de `cenas/som_audio.txt`.

Ordem: o jogo já está aberto com o volume mestre em 0 (`abrir_horizon.sh`); este script cria o sink nulo, manda o stream do container para ele (`audio_nulo.sh ligar horizon`), confere
(`verificar`: todo link de saída vai para `som_nulo`), liga o gravador, só então sobe o volume mestre a 1 e roda os cenários. Uma thread confere a cada segundo, e se algum stream sair do sink
nulo o volume mestre volta a 0 na hora. No fim: volume mestre 0, gravador parado, sink removido.

Uso: H_NAME=horizon h2_som_audio.py CENARIOS LOG_NO_DESKTOP SAIDA_LOG SAIDA.wav_NO_DESKTOP [nome ...]
  CENARIOS         cenas/som_audio.txt
  LOG_NO_DESKTOP   o arquivo para onde o jogo escreve o stderr
  SAIDA_LOG        o arquivo no notebook com os blocos `=== nome` (o mesmo de `h2_som_medida.py`)
  SAIDA.wav        onde o WAV fica, no desktop
Variável: AUDIO_NULO (o caminho do `audio_nulo.sh` no desktop).
"""
import os, subprocess, sys, threading, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_som_medida as m

h = m.h
AUDIO_NULO = os.environ.get("AUDIO_NULO", "~/.cache/rust-mekanism/particulas/audio_nulo.sh")
CONTAINER = os.environ.get("H_NAME", "horizon")


def desktop(args):
    return subprocess.run(["ssh", "desktop", f"{AUDIO_NULO} {args}"], capture_output=True, text=True)


def main():
    cenarios, log, saida, wav = sys.argv[1:5]
    only = sys.argv[5:]
    # O volume mestre em 0 antes de tudo (o jogo abriu assim, e o stream ainda pode estar nas caixas).
    h.ctl("options-set", "soundCategory_master", "0")
    # Só a categoria `block` (o som das máquinas) fica ligada: mobs, clima, ambiente e música entrariam na gravação (o original foi gravado sem eles).
    for categoria in ("music", "record", "weather", "hostile", "neutral", "player", "ambient", "voice", "ui"):
        h.ctl("options-set", f"soundCategory_{categoria}", "0")
    h.ctl("options-set", "soundCategory_block", "1.0")
    ligado = False
    for _ in range(60):
        if "movidos 0" not in desktop(f"ligar {CONTAINER}").stdout and desktop(f"verificar {CONTAINER}").stdout.startswith("ok"):
            ligado = True
            break
        time.sleep(1)
    if not ligado:
        print("o stream do jogo não foi para o sink nulo: nada gravado, volume mestre em 0")
        desktop("remover")
        sys.exit(1)
    print("stream no sink nulo:", desktop(f"verificar {CONTAINER}").stdout.strip(), flush=True)
    # O bloco da máquina precisa de um vizinho sólido para o `place` (o teste das partículas deixou ar embaixo dele).
    h.command(f"setblock {m.POS[0]} {m.POS[1] - 1} {m.POS[2]} minecraft:stone")
    rec = subprocess.Popen(["ssh", "desktop", f"{AUDIO_NULO} gravar {wav} {wav}.pid"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(2)
    parar = threading.Event()

    def vigia():
        while not parar.is_set():
            desktop(f"ligar {CONTAINER}")
            if not desktop(f"verificar {CONTAINER}").stdout.startswith("ok"):
                print("stream fora do sink nulo: volume mestre em 0", flush=True)
                h.ctl("options-set", "soundCategory_master", "0")
            parar.wait(1.0)

    thread = threading.Thread(target=vigia, daemon=True)
    thread.start()
    try:
        h.ctl("options-set", "soundCategory_master", "1.0")
        os.environ["SOM_NULO"] = "1"
        sys.argv = ["h2_som_medida.py", cenarios, log, saida, *only]
        m.main()
    finally:
        h.ctl("options-set", "soundCategory_master", "0")
        parar.set()
        thread.join(timeout=5)
        # Só o gravador deste script (o pid que `gravar` deixou), antes de o sink sumir: outras sessões podem ter o seu.
        subprocess.run(["ssh", "desktop", f"kill -TERM $(cat {wav}.pid)"], capture_output=True)
        try:
            rec.wait(timeout=20)
        except subprocess.TimeoutExpired:
            print("o gravador não parou em 20 s")
        subprocess.run(["ssh", "desktop", f"rm -f {wav}.pid"], capture_output=True)
        desktop("remover")
    print("gravado", wav)


if __name__ == "__main__":
    main()
