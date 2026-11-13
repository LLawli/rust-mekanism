#!/usr/bin/env python3
"""Os setups reais no port (frente `setups-reais`, decisão 196): o mesmo roteiro da `CenaSetups.java` do original, com as mesmas construções (`cenas/setups_reais.txt`) nas mesmas coordenadas.

    H_NAME=horizon2 PASTA=<pasta do game.log> h2_setups_reais.py roteiro SAIDA.jsonl
    H_NAME=horizon2 PASTA=<pasta do game.log> h2_setups_reais.py limpar

O jogo tem de estar no título (`abrir_som_plano.sh titulo`), com a cópia de teste (`enviar_fontes.sh`: `TRANSMITTER_PROBE = true`). O roteiro cria o mundo `SetupsA` (superplano, a semente do
original), monta os cinco setups por comando, sai para o título, abre o mundo (a abertura `abre1`: o que cada rede é, tick a tick, nos primeiros ticks), põe o combustível, a lava, o carbono, o
calor e os itens (o `fluxo`), anda 600 ticks, se afasta 330 blocos (`afasta`), volta (`volta`), sai com tudo andando e abre de novo (`abre2`). Cada janela vira linhas de SAIDA.jsonl no mesmo
formato das da `CenaSetups` (`e`: `tick`, `tag`, `k`, `carregados`, `fila`, `redes`, `transito`), tiradas das sondas do stderr do jogo (`MBE` energia, `MBP` fluido, `MTN` químico, `MBQ` calor,
`MBI` transportador, `MBC` a fila de chunks, `MBT` a passada), e uma linha `marca` com o conteúdo dos aceitadores onde o original grava uma.
"""
import json, os, re, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS)
os.environ.setdefault("H_NAME", "horizon2")
import h2_reset_mundo as base
from h2_reset_mundo import abrir, cmd, ctl, sair, ssh, state, wait

PASTA = base.PASTA
MUNDO = "SetupsA"
SEMENTE = "20261116"
Y = -60
CENA = os.path.join(TOOLS, "..", "cenas", "setups_reais.txt")
CUBOS = (20, 100, 200, 232, 300)
TANQUES = (100, 190)
# O que o stderr do jogo guarda que interessa: a sonda de cada família, a fila de chunks, o `pre` de cada passada e a abertura. Os `tick` dos block entities (milhares por passada) só se contam.
FILTRO = (
    '/^MBT .*"e":"pre"/ {print "MBX " cnt+0; cnt=0; print; next} '
    '/^MBT .*"e":"tick"/ {cnt++; next} '
    "/^(MBE|MBP|MTN|MBQ|MBI|MBC|MBK) / {print; next} "
    '/^MBT .*"e":"(open_ready|estado|reset|owner_tickets|owner_level)"/ {print}'
)


def passadas_do_servidor():
    """Quantas passadas do servidor o jogo já deu (as linhas `MBC` do stderr): o `wait` do `horizon-ctl` conta quadros do cliente, e o port anda a 7 a 12 ticks por segundo no gamescope
    headless, então esperar por quadros não dá o mesmo número de ticks que o original."""
    out = ssh(f"grep -ac '^MBC ' {PASTA}/game.log").strip()
    return int(out) if out.isdigit() else 0


def espera_ticks(n, limite=600):
    """Espera `n` passadas do servidor (ou `limite` segundos)."""
    inicio, fim = passadas_do_servidor(), time.time() + limite
    while passadas_do_servidor() - inicio < n and time.time() < fim:
        time.sleep(1.0)


def secao(nome):
    """As linhas de uma seção de `cenas/setups_reais.txt` (`== nome`), sem comentário nem linha em branco."""
    linhas, dentro = [], False
    for linha in open(CENA):
        if linha.startswith("== "):
            dentro = linha[3:].strip() == nome
        elif dentro and linha.strip() and not linha.startswith("#"):
            linhas.append(linha.strip())
    return linhas


def log_filtrado(inicio):
    """As linhas do stderr do jogo depois da linha `inicio`, só as que a comparação lê (o filtro roda no desktop: o log tem milhares de `tick` por passada)."""
    return ssh(f"tail -n +{inicio + 1} {PASTA}/game.log | awk '{FILTRO}'").splitlines()


def passadas(lines):
    """Agrupa as linhas filtradas por passada do servidor (cada `pre` abre uma). Devolve `(abertura, passadas)`: o `open_ready` e o `estado` (se vieram) e uma lista de dicionários por passada."""
    out, atual, cnt = [], None, None
    for line in lines:
        prefix, _, rest = line.partition(" ")
        if prefix == "MBX":
            if atual is not None:
                atual["ticks"] = int(rest)
            continue
        try:
            row = json.loads(rest) if prefix != "MBX" else None
        except ValueError:
            continue
        if prefix == "MBT":
            if row.get("e") == "open_ready":
                # As passadas de antes de o mundo estar pronto (o carregamento) não contam: a primeira é a que vem depois do `open_ready`.
                out.clear()
                atual = None
            elif row.get("e") == "pre":
                atual = {"mbe": [], "mbp": [], "mtn": [], "mbq": [], "mbi": [], "mbk": [], "mbc": None, "ticks": 0, "runs": True}
                out.append(atual)
            continue
        if atual is None:
            continue
        key = prefix.lower()
        if prefix == "MBC":
            atual["mbc"] = row
        elif prefix == "MBI":
            atual["mbi"].append(row)
        elif prefix == "MBK":
            atual["mbk"].append([row["cx"], row["cz"], row["loaded"]])
        else:
            atual[key].append(row)
            atual["runs"] = atual["runs"] and bool(row.get("runs", True))
    return out


def redes_da_passada(p):
    """As redes de uma passada no formato do original: tipo, transmissores, aceitadores, buffer, capacidade e o que saiu."""
    redes = []
    for linha in p["mbe"]:
        for buf, cap, saiu, tx, acc in linha["nets"]:
            redes.append({"k": "Energy", "tx": tx, "acc": acc, "buf": buf, "cap": cap, "saiu": saiu})
    for linha in p["mbp"]:
        for _res, quantidade, cap, saiu, tx, acc in linha["nets"]:
            redes.append({"k": "Fluid", "tx": tx, "acc": acc, "buf": quantidade, "cap": cap, "saiu": saiu})
    for linha in p["mtn"]:
        for _res, quantidade, cap, saiu, tx, acc in linha["nets"]:
            redes.append({"k": "Chemical", "tx": tx, "acc": acc, "buf": quantidade, "cap": cap, "saiu": saiu})
    for linha in p["mbq"]:
        for temp, perda, troca, tx, acc in linha["nets"]:
            redes.append({"k": "Heat", "tx": tx, "acc": acc, "temp": temp, "perda": perda, "troca": troca})
    redes.sort(key=lambda r: (r["k"] + "Network", -r["tx"]))
    return redes


def transito_da_passada(p):
    """A soma das pilhas em trânsito das linhas `MBI` (cada transportador que andou escreve a dele) da passada."""
    return sum(len(linha["stacks"]) for linha in p["mbi"])


def janela(saida, tag, inicio, ticks):
    """Escreve as `ticks` primeiras passadas (que rodam normalmente) depois da linha `inicio`, no formato da `CenaSetups`."""
    lines = log_filtrado(inicio)
    todas = [p for p in passadas(lines) if p["runs"]]
    for k, p in enumerate(todas[:ticks], 1):
        mbc = p["mbc"] or {}
        saida.write(json.dumps({"e": "tick", "tag": tag, "k": k, "carregados": mbc.get("carregados"), "cheios": mbc.get("carregados"), "ms": mbc.get("ms"), "fila": mbc.get("fila"), "redes": redes_da_passada(p), "transito": transito_da_passada(p), "tickets": p["mbk"],
                                "block_entities": p["ticks"]}) + "\n")
    saida.flush()
    print(tag, "passadas", len(todas), "escritas", min(len(todas), ticks), flush=True)
    return todas


def numero(texto, padrao):
    achou = re.search(padrao, texto)
    return float(achou.group(1)) if achou else None


def le(pos):
    return cmd("data get block %d %d %d" % pos)


def conteudo():
    """O conteúdo dos aceitadores agora (o que a `CenaSetups` grava em cada tick): a energia do gerador e dos cubos, a lava, o carbono e o calor, e os itens dos baús (a leitura vale `-1`
    quando o chunk está descarregado: o comando não acha o bloco)."""
    def energia(pos):
        texto = le(pos)
        if "has the following" not in texto:
            return -1
        # Um contêiner vazio não escreve `energy` no NBT.
        valor = numero(texto, r"energy: (-?[0-9]+)")
        return 0 if valor is None else int(valor)

    def quantidade(pos):
        texto = le(pos)
        valor = numero(texto, r"amount: ([0-9]+)")
        return -1 if "has the following" not in texto and "block data" not in texto else int(valor or 0)

    def calor(pos):
        texto = le(pos)
        valor = numero(texto, r"heat: (-?[0-9.]+(?:E-?[0-9]+)?)")
        return None if valor is None else valor

    def itens(pos):
        texto = le(pos)
        if "has the following" not in texto and "block data" not in texto:
            return -1
        return sum(int(c) for c in re.findall(r"count: ([0-9]+)", texto))

    return {
        "gerador": energia((0, Y, 0)),
        "cubos": [energia((x, Y, 1)) for x in CUBOS],
        "tanques": {
            "lava": [quantidade((x, Y, 7)) for x in TANQUES],
            "carbono": [quantidade((x, Y, 13)) for x in TANQUES],
            "calor": [calor((0, Y, 18))] + [calor((x, Y, 19)) for x in TANQUES],
        },
        "baus": [itens((1, Y + 2, 24)), itens((140, Y, 25))],
    }


def marca(saida, tag):
    cmd("tick freeze")
    time.sleep(1.5)
    row = {"e": "marca", "tag": tag, **conteudo()}
    cmd("tick unfreeze")
    saida.write(json.dumps(row) + "\n")
    saida.flush()
    print("marca", tag, json.dumps(row), flush=True)


def passo_nomeado(nome):
    """O que o original faz pelo Java, o port faz pelo NBT do bloco (a rede e os tanques do teste: `probe_push`)."""
    if nome == "@maquinas":
        # O que a colocação por item dá (decisão 113): `INPUT` nos seis lados da energia da Enrichment Chamber (`config0`) e do químico do Infuser (`config2` e `config3`).
        for x in CUBOS:
            cmd("data merge block %d %d 1 {component_config:{config0:[I;1,1,1,1,1,1]}}" % (x, Y))
        for x in TANQUES:
            cmd("data merge block %d %d 13 {component_config:{config2:[I;1,1,1,1,1,1],config3:[I;1,1,1,1,1,1]}}" % (x, Y))
    elif nome == "@minerio":
        for x in CUBOS:
            cmd('data merge block %d %d 1 {Items:[{Slot:0b,id:"minecraft:iron_ore",count:64}]}' % (x, Y))
    elif nome == "@combustivel":
        cmd('data merge block 0 %d 0 {Items:[{Slot:0b,id:"minecraft:coal",count:64}]}' % Y)
    elif nome == "@lava":
        cmd('data merge block 1 %d 6 {probe_push:{fluid:"minecraft:lava",amount:20000}}' % Y)
    elif nome == "@carbono":
        cmd('data merge block 1 %d 12 {probe_push:{chem:"mekanism:carbon",amount:5000}}' % Y)
    elif nome == "@calor":
        cmd("data merge block 0 %d 18 {heat:30000.0d}" % Y)
    elif nome == "@itens":
        cmd('data merge block 1 %d 24 {Items:[{Slot:0b,id:"minecraft:cobblestone",count:64}]}' % (Y + 2))
    else:
        raise SystemExit("passo desconhecido: " + nome)


def roda(nome):
    for linha in secao(nome):
        if linha.startswith("cmd "):
            cmd(linha[4:])
        elif linha.startswith("espera "):
            espera_ticks(int(linha[7:]))
        elif linha.startswith("@"):
            passo_nomeado(linha)
        else:
            raise SystemExit("linha desconhecida: " + linha)


def criar_mundo():
    for nome in (MUNDO,):
        ctl("world-delete", nome, "--confirm")
    import h2_mundo_plano as mp
    sys.argv = ["h2_mundo_plano.py", MUNDO, SEMENTE]
    mp.main()


def abertura(saida, tag, ticks, espera):
    inicio = base.log_total()
    abrir(MUNDO)
    espera_ticks(espera)
    todas = janela(saida, tag, inicio, ticks)
    marca(saida, "depois_de_abrir" if tag == "abre1" else "depois_de_reabrir")
    return todas


def roteiro(caminho):
    saida = open(caminho, "w")
    if state().get("world") != MUNDO:
        criar_mundo()
    cmd("gamemode creative @s")
    roda("construir")
    marca(saida, "construido")
    sair()
    abertura(saida, "abre1", 80, 220)
    # O fluxo: os cinco passos nomeados e 600 ticks.
    inicio = base.log_total()
    roda("fluxo")
    espera_ticks(630)
    janela(saida, "fluxo", inicio, 600)
    marca(saida, "depois_do_fluxo")
    inicio = base.log_total()
    roda("afasta")
    espera_ticks(320)
    janela(saida, "afasta", inicio, 300)
    marca(saida, "depois_de_afastar")
    inicio = base.log_total()
    roda("volta")
    espera_ticks(320)
    janela(saida, "volta", inicio, 300)
    marca(saida, "depois_de_voltar")
    sair()
    abertura(saida, "abre2", 80, 100)
    sair()
    saida.close()


def main():
    what = sys.argv[1] if len(sys.argv) > 1 else ""
    if what == "roteiro":
        roteiro(sys.argv[2])
    elif what == "limpar":
        ctl("world-delete", MUNDO, "--confirm")
    else:
        raise SystemExit("uso: h2_setups_reais.py roteiro SAIDA.jsonl | limpar")


if __name__ == "__main__":
    main()
