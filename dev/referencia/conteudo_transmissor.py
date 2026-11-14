"""O modelo do conteúdo dos transmissores (`TransmitterContentsManager`, `mekanism:models/transmitter_contents.obj`): o OBJ e o `.mtl` do original, copiados byte a byte para
`models/block/` (o OBJ com o sufixo `.obj.mek`, como os dos transmissores), e um modelo avulso por grupo (`<lado><TIPO>`, 24) com só aquele grupo visível.

O original assa o modelo por conjunto de tipos de conexão (`ModelUtil.partVisibility`) e troca a textura `contents` por instância. O Horizon assa um modelo avulso por nome, com a
textura fixa e a visibilidade fixa, então o port assa os 24 grupos, desenha a união dos que valem para a instância e troca o sprite na hora de desenhar (`machinery_transmitter_contents`).

Uso: `conteudo_transmissor.py [CHECKOUT_DO_MEKANISM]` (o padrão é o checkout do original em /mnt/dados/caches/rust-mekanism-ref/mekanism)."""
import json
import os
import re
import shutil
import sys

AQUI = os.path.dirname(os.path.abspath(__file__))
RAIZ = os.path.abspath(os.path.join(AQUI, "..", ".."))
CHECKOUT = sys.argv[1] if len(sys.argv) > 1 else "/mnt/dados/caches/rust-mekanism-ref/mekanism"
ORIGEM = f"{CHECKOUT}/src/main/resources/assets/mekanism/models"
DESTINO = f"{RAIZ}/mods/mekanism/content/assets/mekanism/models/block"
PASTA = f"{DESTINO}/transmitter_contents"
SPRITE = "mekanism:mek_liquid/energy"


def grupos(obj):
    with open(obj, encoding="utf-8") as arquivo:
        return sorted({linha.split()[1] for linha in arquivo if linha.startswith("g ")})


def main():
    os.makedirs(PASTA, exist_ok=True)
    shutil.copyfile(f"{ORIGEM}/transmitter_contents.obj", f"{DESTINO}/transmitter_contents.obj.mek")
    shutil.copyfile(f"{ORIGEM}/transmitter_contents.mtl", f"{DESTINO}/transmitter_contents.mtl")
    nomes = grupos(f"{DESTINO}/transmitter_contents.obj.mek")
    if len(nomes) != 24:
        raise SystemExit(f"o OBJ do conteúdo devia ter 24 grupos, tem {len(nomes)}")
    # `mtllib` aponta para o `.mtl` ao lado, como nos transmissores.
    texto = open(f"{DESTINO}/transmitter_contents.obj.mek", encoding="utf-8").read()
    if not re.search(r"^mtllib transmitter_contents\.mtl$", texto, re.M):
        raise SystemExit("o OBJ do conteúdo não aponta para transmitter_contents.mtl")
    for nome in nomes:
        modelo = {
            "loader": "neoforge:obj",
            "model": "mekanism:models/block/transmitter_contents.obj.mek",
            "flip_v": True,
            "shade_quads": False,
            "ambientocclusion": False,
            "textures": {"particle": SPRITE, "contents": SPRITE},
            "visibility": {outro: outro == nome for outro in nomes},
        }
        with open(f"{PASTA}/{nome}.json", "w", encoding="utf-8") as arquivo:
            json.dump(modelo, arquivo, indent=2)
            arquivo.write("\n")
    print(f"{len(nomes)} modelos de conteúdo")


if __name__ == "__main__":
    main()
