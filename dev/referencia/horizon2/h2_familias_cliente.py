#!/usr/bin/env python3
"""O condutor e o transportador no cliente e na liga, e a troca de dimensão, no jogo (frente `cliente-familias`, Fase 3, decisões 163 a 165).

    H_NAME=horizon DOIS_DIR=<pasta dos logs a.log e b.log> h2_familias_cliente.py condutor        # o calor no convidado (HEAT_STORED) e a liga num condutor, com o antes e o depois recortados
    H_NAME=horizon DOIS_DIR=... h2_familias_cliente.py transportador                              # as pilhas e a cor no convidado e a liga num transportador, sem soltar nada
    H_NAME=horizon DOIS_DIR=... h2_familias_cliente.py opcao                                      # transmitterAlloyUpgrade = false: a liga passa e não se gasta; true de novo: sobe
    H_NAME=horizon DOIS_DIR=... h2_familias_cliente.py dimensao                                   # o convidado e o anfitrião vão ao Nether e voltam: os transmissores chegam sem pedido novo
    NOME=... h2_familias_cliente.py video                                                         # o vídeo com som da liga num condutor e num transportador

Dois clientes na mesma vaga (`abrir_dois.sh anfitriao spike`, `horizon-ctl lan open 0 commands`, `abrir_dois.sh convidado 127.0.0.1:PORTA Convidado`). O anfitrião age; o convidado observa.
A sonda `MBG` (cópia de teste) ganhou `dim`, `heat`, `color` e `transit` (`[índice, item, quantidade]` por pilha): o que o cliente guardou do pacote de atualização."""
import json, os, re, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon")
import h2_cabo_cliente as c
import h2_ajustes_jogo as aj
import h2_identity as h

cmd, wait, at, txt, log = aj.cmd, aj.wait, aj.at, aj.txt, c.log
CONDUTORES = ["mekanism:%s_thermodynamic_conductor" % t for t in ("basic", "advanced", "elite", "ultimate")]
TRANSPORTADORES = ["mekanism:%s_logistical_transporter" % t for t in ("basic", "advanced", "elite", "ultimate")]
GERADOR = "mekanismgenerators:heat_generator"
LINHA = 5


def blocos_ok(lista, nivel, xs, dz=0):
    ok = True
    for x in xs:
        ok &= "Test passed" in cmd("execute if block %s %s" % (txt(at(x, 0, dz)), lista[nivel]))
    return ok


def ultimo(arquivo, x, dz=0, prefixo="MBG", dim=None):
    rows = [r for r in log(arquivo, prefixo) if r["p"] == list(at(x, 0, dz)) and (dim is None or r.get("dim") == dim)]
    return rows[-1] if rows else None


def condutor():
    aj.clear()
    cmd("gamemode creative @s")
    cmd("tick unfreeze")
    c.observer_near()
    for x in range(LINHA):
        cmd("setblock %s %s" % (txt(at(x)), CONDUTORES[0]))
    cmd("setblock %s %s" % (txt(at(LINHA - 1, 1)), GERADOR))
    wait(60)
    cmd("data merge block %s {heat:8000.0d}" % txt(at(LINHA - 1, 1)))
    wait(300)
    time.sleep(2)
    q = [r for r in log("a", "MBQ")][-1]
    servidor = {tuple(p[:3]): p[3] for p in q["cond"]}
    print("calor no servidor:", [round(servidor[at(x)], 1) for x in range(LINHA)])
    ok = True
    for quem in ("a", "b"):
        linhas = [ultimo(quem, x) for x in range(LINHA)]
        calor = [None if r is None else r["heat"] for r in linhas]
        print("calor no cliente", "(anfitrião, who=guest com a LAN)" if quem == "a" else "(convidado)", calor)
        for x in range(LINHA):
            real = servidor[at(x)]
            visto = calor[x]
            if visto is None or abs(visto - real) > 0.06 * max(real, 1.0):
                ok = False
    print(("OK   " if ok else "DIFF ") + "o calor do cliente fica a menos de um vigésimo do servidor (clientTemperature)")
    mid, eye = aj.camera(LINHA)
    aj.enquadrar(LINHA)
    antes = aj.shot("condutor_antes")
    for nivel in (1, 2, 3):
        c.usar_liga(c.LIGAS[nivel - 1], at(1))
        wait(40)
        time.sleep(1.5)
        good = blocos_ok(CONDUTORES, nivel, range(LINHA))
        ok &= good
        q = [r for r in log("a", "MBQ")][-1]
        guest = ultimo("b", 1)
        print(("OK   " if good else "DIFF "), "nível", nivel, "todos subiram:", good, "| redes:", len(q["nets"]), "condutores na rede:", q["nets"][0][3] if q["nets"] else None, "| convidado (1):", {k: guest[k] for k in ("net", "n", "heat")} if guest else None)
        aj.enquadrar(LINHA)
        depois = aj.shot("condutor_n%d" % nivel)
        print("empilhado:", aj.stack(antes, depois, f"{c.LOCAL}/condutor_antes_n{nivel}.png", "antes (básico)", f"depois (nível {nivel})"))
    print("fim", "OK" if ok else "DIFF")


def montar_transportadores(n=LINHA):
    aj.clear()
    cmd("gamemode creative @s")
    cmd("tick unfreeze")
    c.observer_near(2, -3)
    cmd("setblock %s minecraft:chest" % txt(at(0)))
    cmd("setblock %s minecraft:chest" % txt(at(n + 1)))
    for end in (0, n + 1):
        cmd("data merge block %s {Items:[]}" % txt(at(end)))
    for x in range(1, n + 1):
        cmd("setblock %s %s" % (txt(at(x)), TRANSPORTADORES[0]))
    wait(40)
    cmd("data merge block %s {probe_set:{raw:[I;0,0,0,0,2,0]}}" % txt(at(1)))
    wait(40)


def itens(x):
    out = cmd("data get block %s Items" % txt(at(x)))
    return sum(int(m) for m in re.findall(r"count: (\d+)", out)), out


def transportador():
    montar_transportadores()
    mid, eye = aj.camera(LINHA + 1)
    aj.enquadrar(LINHA + 1)
    antes = aj.shot("transp_antes")
    cmd("data merge block %s {Items:[{Slot:0b,id:\"minecraft:dirt\",count:64}]}" % txt(at(0)))
    wait(24)
    time.sleep(1.5)
    em_voo = [ultimo("b", x) for x in range(1, LINHA + 1)]
    print("pilhas no convidado (último pacote de cada transportador):", [None if r is None else len(r["transit"]) for r in em_voo])
    c.usar_liga(c.LIGAS[0], at(3))
    wait(60)
    time.sleep(1.5)
    ok = blocos_ok(TRANSPORTADORES, 1, range(1, LINHA + 1))
    wait(200)
    dentro, _ = itens(LINHA + 1)
    origem, _ = itens(0)
    chao = cmd("execute if entity @e[type=minecraft:item,distance=..40]")
    print(("OK   " if ok else "DIFF "), "os cinco subiram para avançados:", ok, "| terra no baú de destino:", dentro, "| na origem:", origem, "| entidades de item perto:", chao)
    # Achado em jogo: com a liga no meio do caminho chegam 62 de 64 (sem liga, e com o transportador avançado desde o início, chegam 64): ver a decisão 163.
    print("terra perdida na troca:", 64 - dentro - origem)
    ok &= dentro + origem == 64 and "Test passed" not in chao
    cmd("data merge block %s {probe_set:{color:3}}" % txt(at(2)))
    wait(40)
    time.sleep(1.5)
    cor = ultimo("b", 2)
    print("cor no convidado do transportador 2:", cor and cor["color"], "(esperado 3)")
    ok &= bool(cor) and cor["color"] == 3
    aj.enquadrar(LINHA + 1)
    depois = aj.shot("transp_n1")
    print("empilhado:", aj.stack(antes, depois, f"{c.LOCAL}/transp_antes_n1.png", "antes (básico)", "depois (avançado)"))
    print("fim", "OK" if ok else "DIFF")


def config_arquivo():
    """O caminho do `general.toml` do servidor dentro da pasta de dados do container `horizon`."""
    out = subprocess.run(["ssh", "desktop", "find ~/.local/share/jogo-isolado/horizon/data -path '*Mekanism/general.toml' | head -3"], capture_output=True, text=True).stdout.split()
    return out


def opcao():
    arquivos = config_arquivo()
    print("general.toml:", arquivos)
    montar_transportadores(3)
    c.observer_near(9, -3)  # longe da mira: o jogador na frente do braço recebe o clique no lugar do bloco

    def muda(valor):
        for f in arquivos:
            subprocess.run(["ssh", "desktop", f"sed -i 's/^transmitterAlloyUpgrade = .*/transmitterAlloyUpgrade = {valor}/' {f}"], check=True)
        time.sleep(4)

    try:
        muda("false")
        c.usar_liga(c.LIGAS[0], at(2))
        wait(40)
        parado = blocos_ok(TRANSPORTADORES, 0, range(1, 4))
        seguro = cmd("clear @s mekanism:alloy_infused 0")
        print("com a opção desligada os blocos seguem básicos:", parado, "| a liga na mão:", seguro)
        muda("true")
        c.usar_liga(c.LIGAS[0], at(2))
        wait(40)
        subiu = blocos_ok(TRANSPORTADORES, 1, range(1, 4))
        print("com a opção ligada de novo sobem:", subiu)
        print("fim", "OK" if parado and subiu else "DIFF")
    finally:
        muda("true")


def dimensao():
    aj.clear()
    cmd("gamemode creative @s")
    cmd("tick unfreeze")
    c.observer_near()
    for x in range(LINHA):
        cmd("setblock %s %s" % (txt(at(x)), c.BLOCOS["cabo"][0]))
        cmd("setblock %s %s" % (txt(at(x, 0, 6)), CONDUTORES[0]))
    wait(60)
    time.sleep(1.5)
    antes_b = len(log("b", "MBG"))
    antes_a = len(log("a", "MBG"))
    print("pacotes antes da viagem: convidado", antes_b, "anfitrião (janela)", antes_a)
    ok = True
    for quem, nome in (("Convidado", "b"), ("@s", "a")):
        base = len(log(nome, "MBG"))
        cmd("execute in minecraft:the_nether run tp %s 0 100 0" % quem)
        wait(100)
        no_nether = len(log(nome, "MBG")) - base
        cmd("execute in minecraft:overworld run tp %s %d %d %d" % (quem, at(2)[0], at(2)[1] + 1, at(2)[2] - 3))
        wait(100)
        time.sleep(2)
        rows = log(nome, "MBG")[base:]
        volta = [r for r in rows if r.get("dim") == "minecraft:overworld"]
        posicoes = {tuple(r["p"]) for r in volta}
        esperado = {at(x) for x in range(LINHA)} | {at(x, 0, 6) for x in range(LINHA)}
        completo = esperado <= posicoes
        ok &= completo
        print(("OK   " if completo else "DIFF "), quem, "| pacotes no Nether:", no_nether, "| pacotes da volta:", len(volta), "| transmissores da volta:", len(posicoes), "de", len(esperado))
    print("fim", "OK" if ok else "DIFF")


if __name__ == "__main__":
    {"condutor": condutor, "transportador": transportador, "opcao": opcao, "dimensao": dimensao}[sys.argv[1]]()
