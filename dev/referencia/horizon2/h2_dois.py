#!/usr/bin/env python3
"""Ajudantes dos testes com dois clientes no container `horizon` (decisão 14 do plano): `host(...)` fala com o cliente que abriu o mundo (e o abriu à LAN) e
`guest(...)` com o convidado (socket `/run/user/1000/horizon-control-b.sock`). Os dois clientes sobem com `../abrir_dois.sh`: headless e mudos.

`install_router()` troca `h2_identity.ctl` por um roteador: `command` (e `tick`) vão para o anfitrião, com `@s` trocado pelo nome do convidado, e o resto
(o que um jogador faz: `tp`, `give`, `place`, `interact`, `click-slot`, `screen`, `ui`...) para o convidado. Assim os roteiros de um cliente só (`h2_cursor.py`...)
rodam com o convidado como o jogador."""
import json, os, subprocess, sys

TOOLS = os.path.dirname(os.path.abspath(__file__))
GUEST_SOCKET = "/run/user/1000/horizon-control-b.sock"
GUEST_NAME = "Convidado"
os.environ.setdefault("H_NAME", "horizon")


def _run(socket, *args):
    env = dict(os.environ)
    env.pop("H_SOCKET", None)
    if socket:
        env["H_SOCKET"] = socket
    out = subprocess.run([f"{TOOLS}/ctl.sh", *args], capture_output=True, text=True, timeout=180, env=env)
    return out.stdout


def host(*args):
    return _run(None, *args)


def guest(*args):
    return _run(GUEST_SOCKET, *args)


def host_command(text):
    return json.loads(host("command", text.replace("@s", GUEST_NAME)))["chat"]


def guest_json(*args):
    try:
        return json.loads(guest(*args))
    except json.JSONDecodeError:
        return {"error": "sem json"}


def install_router():
    import h2_identity as h

    def routed(*args):
        if args and args[0] == "tp":
            # O `tp` do ctl do convidado é só do cliente (o servidor o desfaz): o anfitrião teleporta o convidado.
            return host("command", "tp " + GUEST_NAME + " " + " ".join(args[1:]))
        if args and args[0] in ("command", "wait"):
            if args[0] == "command":
                return host("command", args[1].replace("@s", GUEST_NAME))
            return host(*args)
        return guest(*args)

    h.ctl = routed
    h.command = lambda text: json.loads(routed("command", text))["chat"]
