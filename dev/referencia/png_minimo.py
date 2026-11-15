"""Leitor mínimo de PNG, só com a biblioteca padrão: sem entrelaçamento, de 1 a 8 bits, cor indexada, cinza, RGB e RGBA. Serve aos testes que comparam sprites com o que o código embute."""
import struct
import zlib


def ler_png(caminho):
    """Devolve (largura, altura, pixels), com `pixels[y][x]` uma tupla RGBA de 0 a 255."""
    with open(caminho, "rb") as arquivo:
        dados = arquivo.read()
    assert dados[:8] == b"\x89PNG\r\n\x1a\n", "não é um PNG"
    posicao, idat, paleta, transparencia = 8, b"", None, None
    while posicao < len(dados):
        tamanho, tipo = struct.unpack(">I4s", dados[posicao:posicao + 8])
        corpo = dados[posicao + 8:posicao + 8 + tamanho]
        if tipo == b"IHDR":
            largura, altura, profundidade, cor, _, _, entrelacado = struct.unpack(">IIBBBBB", corpo)
            assert entrelacado == 0, "PNG entrelaçado"
        elif tipo == b"PLTE":
            paleta = corpo
        elif tipo == b"tRNS":
            transparencia = corpo
        elif tipo == b"IDAT":
            idat += corpo
        posicao += 12 + tamanho
    canais = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[cor]
    bits = canais * profundidade
    linha_bytes = (largura * bits + 7) // 8
    passo = max(1, bits // 8)
    cru = zlib.decompress(idat)
    anterior = bytearray(linha_bytes)
    linhas = []
    for y in range(altura):
        inicio = y * (linha_bytes + 1)
        filtro, linha = cru[inicio], bytearray(cru[inicio + 1:inicio + 1 + linha_bytes])
        for i in range(linha_bytes):
            esquerda = linha[i - passo] if i >= passo else 0
            cima = anterior[i]
            diagonal = anterior[i - passo] if i >= passo else 0
            if filtro == 1:
                linha[i] = (linha[i] + esquerda) & 255
            elif filtro == 2:
                linha[i] = (linha[i] + cima) & 255
            elif filtro == 3:
                linha[i] = (linha[i] + (esquerda + cima) // 2) & 255
            elif filtro == 4:
                p = esquerda + cima - diagonal
                pa, pb, pc = abs(p - esquerda), abs(p - cima), abs(p - diagonal)
                escolhido = esquerda if pa <= pb and pa <= pc else (cima if pb <= pc else diagonal)
                linha[i] = (linha[i] + escolhido) & 255
        linhas.append(bytes(linha))
        anterior = linha
    pixels = []
    for linha in linhas:
        saida = []
        for x in range(largura):
            if profundidade == 8:
                valores = tuple(linha[x * canais:(x + 1) * canais])
            else:
                bit = x * profundidade
                valores = ((linha[bit // 8] >> (8 - profundidade - bit % 8)) & ((1 << profundidade) - 1),)
            if cor == 3:
                i = valores[0]
                alfa = transparencia[i] if transparencia and i < len(transparencia) else 255
                saida.append((paleta[3 * i], paleta[3 * i + 1], paleta[3 * i + 2], alfa))
            elif cor == 0:
                saida.append((valores[0],) * 3 + (255,))
            elif cor == 2:
                saida.append(tuple(valores) + (255,))
            elif cor == 4:
                saida.append((valores[0],) * 3 + (valores[1],))
            else:
                saida.append(tuple(valores))
        pixels.append(saida)
    return largura, altura, pixels
