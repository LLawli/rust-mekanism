#!/usr/bin/env python3
"""Roda o teste do `Kd` e do `neoforge_TintIndex` do `.mtl`, e das partes do trecho reto (`NONE_CONTIGUOUS`, a girada pelo `transform`), no `horizon-assets` do pin, sem o jogo (frentes `transmissor-obj`, 2026-11-12, e `cabo-modelo`, 2026-11-14).

Uso: obj_horizon_assets.py [--sem-injecao] [PIN_DO_HORIZON] [PASTA_DE_RASCUNHO]

Copia `horizon-core` e `horizon-assets` do pin (`/mnt/dados/caches/rust-mekanism-ref/horizon-pin`) para um workspace de rascunho, põe nele o texto das duas injeções e do módulo de
`mods/machinery_basis/horizon-mixin.toml` (`obj-material`, `obj_material_parse`, `obj_material_quads`) onde o `horizon-mixins` os poria (o módulo no fim de `obj.rs`; o RETURN de `parse_mtl` como um
invólucro que chama a função original e passa o valor ao helper; o HEAD cancelável de `bake_obj` como primeira instrução, e o helper no fim do arquivo), mais as do `transform` de modelo
(`model-transform`, `model_transform_decode`, `model_transform_bake`: a parte girada do trecho reto leva um `transform` de raiz) e roda `testes/horizon_assets_obj.rs` com o conteúdo do Mekanism. É uma conferência da cola e da conta
com o código do Horizon; quem monta o jogo de verdade é o `native-build`. `--sem-injecao` não aplica as injeções: o teste tem de falhar (a mutação). Rode pelo `lim`
(`/mnt/dados/builds/rust-mekanism-ws/lim python3 -B dev/referencia/obj_horizon_assets.py`); apague o rascunho depois.
"""
import os
import re
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "../.."))
ARGS = [a for a in sys.argv[1:] if not a.startswith("--")]
INJETAR = "--sem-injecao" not in sys.argv
PIN = ARGS[0] if ARGS else "/mnt/dados/caches/rust-mekanism-ref/horizon-pin"
SCRATCH = ARGS[1] if len(ARGS) > 1 else os.path.join(os.path.dirname(ROOT), os.path.basename(ROOT) + "-scratch/obj")
MOD = os.path.join(ROOT, "mods/machinery_basis")

BAKE_OBJ_HEAD = "    output: &mut Vec<ModelQuad>,\n) -> Result<()> {\n    let render_type = model.render_type.as_deref().and_then(RenderType::by_name);\n    for face in obj.faces.iter()"
BAKE_INTO_HEAD = "    output: &mut Vec<ModelQuad>,\n) -> Result<()> {\n    let render_type = model.render_type.as_deref().and_then(RenderType::by_name);\n    for element in model.elements() {"
BAKE_INTO_CALL = "    output: &mut Vec<ModelQuad>,\n) -> Result<()> {\n    if let core::ops::ControlFlow::Break(v) = machinery_model_transform_bake(model, transform, skip_flat, &mut *output) {\n        return v;\n    }\n    let render_type = model.render_type.as_deref().and_then(RenderType::by_name);\n    for element in model.elements() {"
BAKE_OBJ_CALL = "    output: &mut Vec<ModelQuad>,\n) -> Result<()> {\n    if let core::ops::ControlFlow::Break(v) = machinery_obj_material_quads(model, obj, flip_v, shade, hidden, transform, &mut *output) {\n        return v;\n    }\n    let render_type = model.render_type.as_deref().and_then(RenderType::by_name);\n    for face in obj.faces.iter()"


def read(path):
    with open(path) as handle:
        return handle.read()


def write(path, text):
    with open(path, "w") as handle:
        handle.write(text)


def patch_once(text, marker, replacement):
    assert text.count(marker) == 1, marker
    return text.replace(marker, replacement)


def main():
    shutil.rmtree(SCRATCH, ignore_errors=True)
    os.makedirs(f"{SCRATCH}/crates")
    for crate in ("horizon-core", "horizon-assets"):
        shutil.copytree(f"{PIN}/crates/{crate}", f"{SCRATCH}/crates/{crate}", ignore=shutil.ignore_patterns("bin", "examples", "tests"))
    shutil.copy(f"{PIN}/Cargo.lock", SCRATCH)
    pin = read(f"{PIN}/Cargo.toml")
    start, lints = pin.index("[workspace.package]"), pin.index("[workspace.lints.clippy]")
    end = pin.index("\n[", lints + 10)
    write(f"{SCRATCH}/Cargo.toml", '[workspace]\nmembers = ["crates/horizon-core", "crates/horizon-assets"]\nresolver = "2"\n\n' + pin[start:lints].split("\n# Ports mirror")[0] + "\n" + pin[lints:end] + "\n")
    core = f"{SCRATCH}/crates/horizon-core/Cargo.toml"
    write(core, re.sub(r"\[\[bin\]\].*?(?=\n\[\[bin\]\]|\n\[lints\])", "", read(core), flags=re.S))
    assets = f"{SCRATCH}/crates/horizon-assets/Cargo.toml"
    write(assets, read(assets).replace('[dev-dependencies]\npng = "0.18"\n', ""))
    src = f"{SCRATCH}/crates/horizon-assets/src"
    if INJETAR:
        obj = patch_once(read(f"{src}/obj.rs"), "pub fn parse_mtl(text: &str) -> BTreeMap<String, MtlTexture> {", "fn parse_mtl_original(text: &str) -> BTreeMap<String, MtlTexture> {")
        at = obj.index("#[cfg(test)]")
        wrapper = "\npub fn parse_mtl(text: &str) -> BTreeMap<String, MtlTexture> {\n    let value = parse_mtl_original(text);\n    machinery_obj_material_parse(value, text)\n}\n\n"
        write(f"{src}/obj.rs", obj[:at] + "pub mod machinery_obj_material {\n" + read(f"{MOD}/obj_material.rs") + "\n}\n" + read(f"{MOD}/obj_material_parse.rs") + wrapper + obj[at:])
        bake = patch_once(read(f"{src}/bake.rs"), BAKE_OBJ_HEAD, BAKE_OBJ_CALL)
        # O `transform` de raiz do NeoForge (a parte girada do trecho reto): o módulo no fim de `math.rs`, o MODIFY_VARIABLE de `CuboidModel::decode` e o HEAD cancelável de `bake_into`.
        write(f"{src}/math.rs", read(f"{src}/math.rs") + "\npub(crate) mod machinery_model_transform {\n" + read(f"{MOD}/model_transform.rs") + "\n}\n")
        model = patch_once(
            read(f"{src}/model.rs"),
            "        let mut textures = BTreeMap::new();\n        if let Some(raw) = obj.get(\"textures\")",
            "        let mut textures = machinery_model_transform_decode(raw, BTreeMap::new());\n        if let Some(raw) = obj.get(\"textures\")",
        )
        write(f"{src}/model.rs", model + "\n" + read(f"{MOD}/model_transform_decode.rs"))
        bake = patch_once(bake, BAKE_INTO_HEAD, BAKE_INTO_CALL)
        write(f"{src}/bake.rs", bake + "\n" + read(f"{MOD}/obj_material_quads.rs") + "\n" + read(f"{MOD}/model_transform_bake.rs"))
    # O cargo compara o mtime: a cópia guarda o do pin, e uma rodada sem as injeções reaproveitaria o build da rodada com elas.
    for folder, _, names in os.walk(SCRATCH):
        for name in names:
            os.utime(os.path.join(folder, name))
    os.makedirs(f"{SCRATCH}/crates/horizon-assets/tests")
    shutil.copy(f"{HERE}/testes/horizon_assets_obj.rs", f"{SCRATCH}/crates/horizon-assets/tests/obj.rs")
    env = dict(os.environ, CONTENT=f"{ROOT}/mods/mekanism/content", RASTROS=f"{ROOT}/dev/referencia/rastros")
    sys.exit(subprocess.call(["cargo", "test", "--offline", "-p", "horizon-assets", "--test", "obj"], cwd=SCRATCH, env=env))


if __name__ == "__main__":
    main()
