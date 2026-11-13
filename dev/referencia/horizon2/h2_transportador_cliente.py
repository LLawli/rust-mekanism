#!/usr/bin/env python3
"""As pendências do transportador no cliente, em jogo (frente `transportador-cliente`, Fase 3, decisões 169 a 171).

    H_NAME=horizon DOIS_DIR=<pasta dos logs a.log e b.log> h2_transportador_cliente.py liga [rodadas] [espera]   # a liga numa linha de transportadores com terra em trânsito: onde cada unidade está no fim
    H_NAME=horizon DOIS_DIR=... h2_transportador_cliente.py controle [rodadas]                                    # o mesmo roteiro sem a liga (64 chegam)
    H_NAME=horizon DOIS_DIR=... h2_transportador_cliente.py pilhas                                                # o convidado e o servidor: a lista de pilhas de cada transportador, tick a tick
    H_NAME=horizon DOIS_DIR=... h2_transportador_cliente.py chunks                                                # o jogador anda para longe e volta: os tiles que o cliente guarda
    H_NAME=horizon DOIS_DIR=... h2_transportador_cliente.py dobro [rodadas]                                       # a liga num condutor com o convidado conectado: os eventos de troca de bloco em dobro?
    NOME=... h2_transportador_cliente.py video                                                                    # o vídeo com som da liga numa linha com terra em trânsito

Um cliente (o anfitrião, sem LAN) para `liga` e `controle`; dois (`abrir_dois.sh anfitriao spike`, `horizon-ctl lan open 0 commands`, `abrir_dois.sh convidado 127.0.0.1:PORTA Convidado`) para os outros.
A sonda (`TRANSMITTER_PROBE`, `enviar_fontes.sh`): `MBI` as pilhas de um transportador a cada tick dele, `MBL` os passos da liga, `MBX` o que o espelho de um baú aplicou."""
import json, os, re, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon")
import h2_cabo_cliente as c
import h2_ajustes_jogo as aj
import h2_identity as h

cmd, wait, at, txt, log = aj.cmd, aj.wait, aj.at, aj.txt, c.log
TRANSPORTADORES = ["mekanism:%s_logistical_transporter" % t for t in ("basic", "advanced", "elite", "ultimate")]
LINHA = 5


def total(texto, chave):
    return sum(int(m) for m in re.findall(chave + r": (\d+)", texto))


def montar(n=LINHA):
    aj.clear()
    cmd("gamemode creative @s")
    cmd("tick unfreeze")
    cmd("setblock %s minecraft:chest" % txt(at(0)))
    cmd("setblock %s minecraft:chest" % txt(at(n + 1)))
    for end in (0, n + 1):
        cmd("data merge block %s {Items:[]}" % txt(at(end)))
    for x in range(1, n + 1):
        cmd("setblock %s %s" % (txt(at(x)), TRANSPORTADORES[0]))
    wait(40)
    cmd("data merge block %s {probe_set:{raw:[I;0,0,0,0,2,0]}}" % txt(at(1)))
    wait(40)


def no_chao():
    """As unidades de terra soltas no mundo: lê o `Item` da entidade mais perto e a mata, até não sobrar nenhuma."""
    soma = 0
    for _ in range(40):
        out = cmd("data get entity @n[type=minecraft:item,distance=..40] Item")
        m = re.search(r"count: (\d+)", out)
        if not m:
            break
        soma += int(m.group(1))
        cmd("kill @n[type=minecraft:item,distance=..40]")
    return soma


def em_transito(n=LINHA):
    """As pilhas de cada transportador pela última linha `MBI` dele (a sonda grava a cada tick do transportador)."""
    ultimo = {}
    for r in log("a", "MBI"):
        ultimo[tuple(r["p"])] = r
    return [sum(s[2] for s in ultimo[at(x)]["stacks"]) if at(x) in ultimo else None for x in range(1, n + 1)]


def contagem(n=LINHA):
    """Onde está cada unidade de terra: o baú de origem, o de destino, as pilhas dentro de cada transportador e as soltas no mundo."""
    origem = total(cmd("data get block %s Items" % txt(at(0))), "count")
    destino = total(cmd("data get block %s Items" % txt(at(n + 1))), "count")
    return origem, destino, em_transito(n), no_chao()


def uma_rodada(com_liga=True, espera=24, alvo=3, depois=700):
    no_chao()
    montar()
    no_chao()
    inicio = bytes_de("a")
    cmd("data merge block %s {Items:[{Slot:0b,id:\"minecraft:dirt\",count:64}]}" % txt(at(0)))
    wait(espera)
    em_voo = em_transito()
    if com_liga:
        c.usar_liga(c.LIGAS[0], at(alvo))
        wait(30)
    wait(depois)
    # O estado de agora: o servidor parado e um tick, para cada transportador gravar uma linha `MBI`.
    cmd("tick freeze")
    cmd("tick step 1")
    time.sleep(1.5)
    linhas, _ = novas("a", inicio, {"MBI", "MBX", "MBD", "MBL", "MBT"})
    cmd("tick unfreeze")
    ultimo = {}
    for prefixo, r in linhas:
        if prefixo == "MBI":
            ultimo[tuple(r["p"])] = r
    dentro = [sum(s[2] for s in ultimo[at(x)]["stacks"]) if at(x) in ultimo else None for x in range(1, LINHA + 1)]
    puts = sum(r["put"] for prefixo, r in linhas if prefixo == "MBX" and "put" in r and r["p"] == list(at(LINHA + 1)))
    takes = sum(r["take"] for prefixo, r in linhas if prefixo == "MBX" and "take" in r and r["p"] == list(at(0)))
    caidas = [(r["p"][0], r["n"]) for prefixo, r in linhas if prefixo == "MBD"]
    passos = [(r["p"][0], r["given"], r["carried"]) for prefixo, r in linhas if prefixo == "MBL" and r.get("e") == "tstep"]
    remocoes = {}
    for prefixo, r in linhas:
        if prefixo == "MBT" and r.get("e") in ("setRemoved", "clearRemoved") and r["p"][0] == at(3)[0]:
            remocoes.setdefault(r["e"], []).append(r["t"])
    origem = total(cmd("data get block %s Items" % txt(at(0))), "count")
    destino = total(cmd("data get block %s Items" % txt(at(LINHA + 1))), "count")
    soltas = no_chao()
    perdidas = 64 - origem - destino - sum(d or 0 for d in dentro) - soltas
    print("em voo", em_voo, "(%d)" % sum(d or 0 for d in em_voo), "| origem", origem, "destino", destino, "dentro", dentro, "soltas", soltas, "| tiradas da origem", takes, "postas no destino", puts, "| passos da liga (x, dado, carregadas)", passos, "| quedas MBD (x, n)", caidas, "| eventos do bloco 3", remocoes, "| perdidas:", perdidas)
    return destino, perdidas


def liga():
    rodadas = int(sys.argv[2]) if len(sys.argv) > 2 else 3
    espera = int(sys.argv[3]) if len(sys.argv) > 3 else 24
    for i in range(rodadas):
        print("rodada", i + 1)
        uma_rodada(True, espera)


def controle():
    rodadas = int(sys.argv[2]) if len(sys.argv) > 2 else 2
    for i in range(rodadas):
        print("controle", i + 1)
        uma_rodada(False)


def bytes_de(arquivo):
    """O tamanho do log de `arquivo` (a.log do anfitrião, b.log do convidado), para ler só o que vier depois."""
    out = subprocess.run(["ssh", "desktop", f"stat -c %s {c.DOIS}/{arquivo}.log"], capture_output=True, text=True, timeout=60).stdout.strip()
    return int(out or 0)


def novas(arquivo, de, prefixos):
    """As linhas `prefixos` do log escritas depois do byte `de`, como `(prefixo, json)`, e o novo tamanho."""
    out = subprocess.run(["ssh", "desktop", f"tail -c +{de + 1} {c.DOIS}/{arquivo}.log | grep -a '^M' "], capture_output=True, text=True, timeout=120).stdout
    linhas = []
    for linha in out.splitlines():
        prefixo, _, resto = linha.partition(" ")
        if prefixo in prefixos:
            try:
                linhas.append((prefixo, json.loads(resto)))
            except ValueError:
                pass
    return linhas, de + len(out.encode())


def pilhas(ticks=150):
    """O convidado e o servidor, tick a tick: com o servidor parado, cada `tick step 1` anda uma vez; depois dele, a lista de pilhas de cada transportador no convidado (o que `MBH` e `MBG`
    guardaram dos pacotes de movimento e de atualização) é a do servidor ao fim do `Update` do próprio transportador (`MBI`)."""
    montar()
    c.observer_near(2, -3)
    cmd("data merge block %s {Items:[{Slot:0b,id:\"minecraft:dirt\",count:64}]}" % txt(at(0)))
    cmd("tick freeze")
    pos = [at(x) for x in range(1, LINHA + 1)]
    guest = {tuple(p): [] for p in pos}
    de_a, de_b = bytes_de("a"), bytes_de("b")
    diffs, compared, empty_ticks = [], 0, 0
    packets = {"sync": 0, "batch": 0}
    for passo in range(1, ticks + 1):
        cmd("tick step 1")
        time.sleep(0.9)
        linhas_a, de_a = novas("a", de_a, {"MBI"})
        linhas_b, de_b = novas("b", de_b, {"MBH", "MBG"})
        for prefixo, r in linhas_b:
            if prefixo == "MBH":
                packets[r["k"]] += 1
            if tuple(r["p"]) in guest:
                guest[tuple(r["p"])] = sorted((t[0], t[1], t[2]) for t in r["transit"])
        servidor = {}
        for _, r in linhas_a:
            if tuple(r["p"]) in guest:
                servidor[tuple(r["p"])] = sorted((s[0], s[1], s[2]) for s in r["stacks"])
        for p, want in servidor.items():
            compared += 1
            have = guest[p]
            if want != have:
                diffs.append((passo, p, want, have))
            if not want:
                empty_ticks += 1
    cmd("tick unfreeze")
    print("pacotes no convidado:", packets, "| estados de transportador comparados:", compared, "(sem pilha:", empty_ticks, ")")
    print(("OK   " if not diffs else "DIFF ") + "a lista do convidado é a do servidor a cada tick" + ("" if not diffs else " | primeiras diferenças: %s" % diffs[:3]))
    return not diffs


def chunks():
    """O jogador anda para longe e volta: o `ChunkMap` para de mandar os chunks que saem da vista e o cliente larga os tiles deles (`MBF`: quantos chunks o servidor mandou esquecer e
    quantos tiles o cliente ainda tem na dimensão); na volta os pacotes de atualização de cada transmissor chegam de novo (`MBG`). O anfitrião (a janela) e o convidado, um de cada vez; depois
    o Nether."""
    montar()
    c.observer_near(2, -3)
    cmd("tick unfreeze")
    wait(60)
    time.sleep(2)
    posicoes = {at(x) for x in range(1, LINHA + 1)}
    ok = True
    for quem, nome, alvo, voltar in (("Convidado", "b", "tp Convidado %d %d %d" % (BASE_LONGE[0], 150, BASE_LONGE[2]), "tp Convidado %d %d %d" % (at(2)[0], at(2)[1] + 1, at(2)[2] - 3)), ("@s", "a", "tp @s %d %d %d" % (BASE_LONGE[0], 150, BASE_LONGE[2]), "tp @s %d %d %d" % (at(2)[0], at(2)[1] + 1, at(2)[2] - 3))):
        antes = len(log(nome, "MBG"))
        base = len(log(nome, "MBF"))
        cmd(alvo)
        wait(120)
        time.sleep(3)
        forget = log(nome, "MBF")[base:]
        resta = forget[-1]["tiles"] if forget else None
        print(quem, "foi longe: MBF", [(f["chunks"], f["tiles"]) for f in forget], "| tiles do cliente depois:", resta)
        ok &= bool(forget) and resta == 0
        volta0 = len(log(nome, "MBG"))
        cmd(voltar)
        wait(120)
        time.sleep(3)
        rows = log(nome, "MBG")[volta0:]
        vistos = {tuple(r["p"]) for r in rows}
        todos = posicoes <= vistos
        print(quem, "voltou: pacotes de atualização", len(rows), "| os 5 transportadores de volta:", todos)
        ok &= todos
    base = len(log("b", "MBF"))
    cmd("execute in minecraft:the_nether run tp Convidado 0 100 0")
    wait(100)
    time.sleep(2)
    forget = log("b", "MBF")[base:]
    print("Convidado no Nether: MBF", [(f["dim"], f["chunks"], f["tiles"]) for f in forget])
    ok &= any(f["dim"] == "minecraft:overworld" and f["chunks"] == -1 and f["tiles"] == 0 for f in forget)
    cmd("execute in minecraft:overworld run tp Convidado %d %d %d" % (at(2)[0], at(2)[1] + 1, at(2)[2] - 3))
    wait(100)
    print("fim", "OK" if ok else "DIFF")
    return ok


BASE_LONGE = (aj.BASE[0] + 480, 150, aj.BASE[2] + 16)


if __name__ == "__main__":
    {"liga": liga, "controle": controle, "pilhas": pilhas, "chunks": chunks}[sys.argv[1]]()
