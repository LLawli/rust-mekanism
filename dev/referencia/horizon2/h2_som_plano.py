#!/usr/bin/env python3
"""Grava o áudio do port (`horizon`) em condições controladas, pelo sink nulo, com o roteiro de `cenas/som_plano.txt` (o mesmo de `testes/CenaSomPlano.java` no original).

O jogo já está aberto no mundo superplano de teste (`h2_mundo_plano.py` o cria; `abrir_horizon.sh` o reabre), com o volume mestre em 0. Este script: fixa o mundo (dificuldade pacífico, `advance_time`,
`advance_weather` e `spawn_mobs` desligados, a hora do arquivo, tempo limpo, criativo), deixa só a categoria `block` ligada, manda o stream do container para o sink nulo (`audio_nulo.sh`) e confere,
liga o gravador, sobe o volume mestre a 1 (um vigia confere a cada segundo e volta a 0 se o stream sair do sink) e roda os cenários: a máquina por `fill` com `facing` explícito, o jogador por `tp`
com yaw e pitch explícitos, a energia e o carvão por `data merge`. Escreve um `.jsonl` com o mundo, a posição real dos olhos (`horizon-ctl state` e `data get entity @s Pos`) e a distância ao centro da máquina.

Uso: H_NAME=horizon h2_som_plano.py CENA SAIDA.jsonl WAV_NO_DESKTOP
Variáveis: AUDIO_NULO (o `audio_nulo.sh` no desktop), SOM_PLANO_ENERGIA (energia posta ao ligar; o padrão é a capacidade lida do bloco).
"""
import json, os, re, shlex, subprocess, sys, threading, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

AUDIO_NULO = os.environ.get("AUDIO_NULO", "~/.cache/rust-mekanism/som-plano/audio_nulo.sh")
CONTAINER = os.environ.get("H_NAME", "horizon")
CONFIG = "~/.local/share/jogo-isolado/horizon/data/horizon/config/Mekanism/client.toml"
TICK = 0.05
CATEGORIAS = ("music", "record", "weather", "hostile", "neutral", "player", "ambient", "voice", "ui")


def ssh(texto):
    return subprocess.run(["ssh", "desktop", texto], capture_output=True, text=True).stdout


def desktop(args):
    return subprocess.run(["ssh", "desktop", f"{AUDIO_NULO} {args}"], capture_output=True, text=True)


def parse(caminho):
    """(mundo, [cenários]) do arquivo da cena."""
    mundo = {"seed": 20261108, "maquina": (4, -60, 4), "facing": "south", "pes": (4.5, -60.0, 1.5), "yaw": 0.0, "pitch": 0.0, "hora": 6000}
    cenarios = []
    for linha in open(caminho, encoding="utf-8"):
        linha = linha.strip()
        if not linha or linha.startswith("#"):
            continue
        if linha.startswith("mundo "):
            for palavra in linha.split()[1:]:
                k, _, v = palavra.partition("=")
                if k == "seed":
                    mundo[k] = int(v)
                elif k in ("maquina",):
                    mundo[k] = tuple(int(float(p)) for p in v.split(","))
                elif k == "pes":
                    mundo[k] = tuple(float(p) for p in v.split(","))
                elif k in ("yaw", "pitch"):
                    mundo[k] = float(v)
                elif k == "hora":
                    mundo[k] = int(v)
                elif k == "facing":
                    mundo[k] = v
            continue
        sc = {"nome": "x", "muffling": 0, "base": 1.0, "enable": True, "liga": 20, "desliga": 200, "fim": 260, "eventos": []}
        for palavra in linha.split():
            k, _, v = palavra.partition("=")
            if k in ("muffling", "liga", "desliga", "fim"):
                sc[k] = int(v)
            elif k == "base":
                sc[k] = float(v)
            elif k == "enable":
                sc[k] = v == "true"
            elif k == "ev":
                sc["eventos"].append(v.split(":", 2))
            elif k == "nome":
                sc[k] = v
        cenarios.append(sc)
    return mundo, cenarios


def config_text(base, enable):
    return (
        "#Settings for configuring Mekanism's Sounds\n[sounds]\n\t#Adjust Mekanism sounds' base volume. < 1 is softer, higher is louder.\n"
        f"\t# Default: 1.0\n\t# Range: 0.0 ~ 10.0\n\tbaseVolume = {base!r}\n\t#Play sounds for active machines.\n\tenableMachine = {str(enable).lower()}\n"
    )


def write_config(base, enable):
    ssh(f"mkdir -p $(dirname {CONFIG}) && printf %s {shlex.quote(config_text(base, enable))} > {CONFIG}")


def estado():
    """O `horizon-ctl state` (JSON) do jogador."""
    try:
        return json.loads(h.ctl("state")).get("player") or {}
    except json.JSONDecodeError:
        return {}


def pos_entidade():
    """`data get entity @s Pos` (a posição do servidor, em doubles)."""
    chat = h.command("data get entity @s Pos")[0]
    nums = re.findall(r"-?\d+(?:\.\d+)?(?=d)", chat)
    return [float(n) for n in nums[:3]]


def distancia(olhos, maquina):
    c = (maquina[0] + 0.5, maquina[1] + 0.5, maquina[2] + 0.5)
    return sum((olhos[i] - c[i]) ** 2 for i in range(3)) ** 0.5


class Cena:
    def __init__(self, mundo, saida):
        self.mundo, self.saida = mundo, saida
        self.maquina = mundo["maquina"]
        self.pos = " ".join(map(str, self.maquina))

    def log(self, **kw):
        self.saida.write(json.dumps(kw, ensure_ascii=False) + "\n")
        self.saida.flush()

    def fixa_mundo(self):
        m = self.mundo
        for cmd in ("difficulty peaceful", "gamerule advance_time false", "gamerule advance_weather false", "gamerule spawn_mobs false", f"time set {m['hora']}", "weather clear", "gamemode creative @s", "clear @s"):
            self.log(e="comando", cmd=cmd, chat=h.command(cmd))
        time.sleep(1.0)
        x, y, z = self.maquina
        info = {"seed": h.command("seed"), "advance_time": h.command("gamerule advance_time"), "advance_weather": h.command("gamerule advance_weather"), "spawn_mobs": h.command("gamerule spawn_mobs"),
                "dificuldade": h.command("difficulty"), "hora": h.command("time query time"), "chao": [h.command(f"execute if block {x} {y - 1} {z} minecraft:grass_block")[0],
                                                                                                       h.command(f"execute if block {x} {y - 2} {z} minecraft:dirt")[0],
                                                                                                       h.command(f"execute if block {x} {y - 4} {z} minecraft:bedrock")[0]]}
        self.log(e="mundo", maquina=list(self.maquina), pes=list(m["pes"]), yaw=m["yaw"], pitch=m["pitch"], **info)

    def pe(self):
        m = self.mundo
        h.command(f"tp @s {m['pes'][0]} {m['pes'][1]} {m['pes'][2]} {m['yaw']} {m['pitch']}")

    def posicao(self, rotulo):
        st = estado()
        olhos = st.get("eye") or st.get("eye_position")
        pos = pos_entidade()
        entrada = {"e": "posicao", "rotulo": rotulo, "state": {k: st.get(k) for k in ("position", "eye", "yaw", "pitch", "on_ground", "gamemode", "time", "weather") if k in st}, "pos": pos}
        if isinstance(olhos, list) and len(olhos) >= 3:
            entrada["distancia"] = distancia(olhos, self.maquina)
        self.log(**entrada)
        return entrada

    def energia(self, ligar):
        if not ligar:
            h.command(f"data merge block {self.pos} {{energy:0}}")
            return
        energia = int(os.environ.get("SOM_PLANO_ENERGIA", "20000"))
        h.command(f'data merge block {self.pos} {{energy:{energia},Items:[{{Slot:0b,id:"minecraft:coal",count:64}}]}}')

    def lit(self):
        out = h.ctl("block", *map(str, self.maquina))
        m = re.search(r"lit=(true|false)", out)
        return m.group(1) if m else out.strip()[:80]

    def add_muffling(self):
        h.command('give @s mekanism:upgrade[mekanism:upgrade_type="mekanism:muffling"] 1')
        time.sleep(0.4)
        h.ctl("key", "shift", "down")
        h.ctl("interact", *map(str, self.maquina))
        h.ctl("key", "shift", "up")

    def roda(self, sc, indice):
        x, y, z = self.maquina
        write_config(sc["base"], sc["enable"])
        time.sleep(2.5)
        h.command("tick unfreeze")
        h.command(f"fill {x - 8} {y} {z - 8} {x + 8} {y + 10} {z + 8} minecraft:air")
        h.command(f"fill {x} {y} {z} {x} {y} {z} mekanism:enrichment_chamber[facing={self.mundo['facing']}]")
        h.ctl("close")
        time.sleep(1.5)
        h.command(f"data merge block {self.pos} {{energy:0}}")
        self.pe()
        time.sleep(1.5)
        self.log(e="cenario", nome=sc["nome"], indice=indice, base=sc["base"], muffling=sc["muffling"], hora_relogio=time.strftime("%H:%M:%S"))
        self.posicao(f"{sc['nome']}#{indice}:antes")
        t0 = time.monotonic()
        pendentes = sorted([(int(e[0]) * TICK, e) for e in sc["eventos"]] + [(sc["liga"] * TICK, ["", "liga", ""]), (sc["desliga"] * TICK, ["", "desliga", ""]),
                                                                          (sc["liga"] * TICK + 4.0, ["", "meio", ""])], key=lambda p: p[0])
        for quando, ev in pendentes:
            espera = t0 + quando - time.monotonic()
            if espera > 0:
                time.sleep(espera)
            que, valor = ev[1], ev[2]
            if que == "liga":
                self.energia(True)
                self.log(e="evento", que="liga", t=time.monotonic() - t0)
            elif que == "desliga":
                self.energia(False)
                self.log(e="evento", que="desliga", t=time.monotonic() - t0, lit=self.lit())
            elif que == "meio":
                self.posicao(f"{sc['nome']}#{indice}:meio")
                self.log(e="lit", t=time.monotonic() - t0, lit=self.lit())
            elif que == "muffling":
                self.add_muffling()
                self.log(e="evento", que="muffling", t=time.monotonic() - t0)
                self.pe()
            elif que == "base":
                write_config(float(valor), sc["enable"])
        espera = t0 + sc["fim"] * TICK - time.monotonic()
        if espera > 0:
            time.sleep(espera)
        h.command(f"fill {x} {y} {z} {x} {y} {z} minecraft:air")
        time.sleep(1.0)
        self.log(e="cenario_fim", nome=sc["nome"])


def main():
    caminho_cena, caminho_saida, wav = sys.argv[1:4]
    mundo, cenarios = parse(caminho_cena)
    saida = open(caminho_saida, "w", encoding="utf-8")
    cena = Cena(mundo, saida)
    antes = ssh(f"cat {CONFIG} 2>/dev/null")
    h.ctl("options-set", "soundCategory_master", "0")
    for categoria in CATEGORIAS:
        h.ctl("options-set", f"soundCategory_{categoria}", "0")
    h.ctl("options-set", "soundCategory_block", "1.0")
    # A mesma configuração de áudio de cliente do original: dispositivo padrão e áudio direcional desligado.
    h.ctl("options-set", "directionalAudio", "false")
    cena.log(e="opcoes", opcoes={k: h.ctl("options-get", k).strip() for k in ("soundDevice", "directionalAudio", "soundCategory_master", "soundCategory_block", "guiScale")})
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
    ssh(f"{AUDIO_NULO} info {CONTAINER} > {wav}.stream.txt")
    h.command("gamemode creative @s")
    cena.fixa_mundo()
    # O chão da máquina é o do superplano; o jogador chega ao ponto fixo antes de o gravador subir.
    cena.pe()
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
        for i, sc in enumerate(cenarios):
            print(sc["nome"], i, flush=True)
            cena.roda(sc, i)
    finally:
        h.ctl("options-set", "soundCategory_master", "0")
        parar.set()
        thread.join(timeout=5)
        subprocess.run(["ssh", "desktop", f"kill -TERM $(cat {wav}.pid)"], capture_output=True)
        try:
            rec.wait(timeout=20)
        except subprocess.TimeoutExpired:
            print("o gravador não parou em 20 s")
        subprocess.run(["ssh", "desktop", f"rm -f {wav}.pid"], capture_output=True)
        desktop("remover")
        ssh(f"printf %s {shlex.quote(antes)} > {CONFIG}")
        saida.close()
    print("gravado", wav)


if __name__ == "__main__":
    main()
