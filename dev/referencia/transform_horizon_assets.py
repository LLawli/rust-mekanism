#!/usr/bin/env python3
"""Roda o teste do `transform` de modelo no `horizon-assets` do pin, sem o jogo (frente `transform-modelo`, 2026-11-09).

Uso: transform_horizon_assets.py [PIN_DO_HORIZON] [PASTA_DE_RASCUNHO]

Copia `horizon-core` e `horizon-assets` do pin (`/mnt/dados/caches/rust-mekanism-ref/horizon-pin`) para um workspace de rascunho, põe nele o texto das duas injeções e do módulo de
`mods/machinery_basis/horizon-mixin.toml` (`model-transform`, `model_transform_decode`, `model_transform_bake`) no lugar onde o `horizon-mixins` os poria (o módulo no fim de
`math.rs`, o MODIFY_VARIABLE do `let mut textures` de `CuboidModel::decode`, o HEAD cancelável como primeira instrução de `bake_into`, o helper no fim do arquivo) e roda `testes/horizon_assets_transform.rs` com o conteúdo do
Generators. É uma conferência da cola e da conta com o código do Horizon; quem monta o jogo de verdade é o `native-build`, que aplica as injeções pelo `horizon-mixins`.
Rode pelo `lim` (`/mnt/dados/builds/rust-mekanism-ws/lim python3 -B dev/referencia/transform_horizon_assets.py`); apague o rascunho depois.
"""
import os
import re
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "../.."))
PIN = sys.argv[1] if len(sys.argv) > 1 else "/mnt/dados/caches/rust-mekanism-ref/horizon-pin"
SCRATCH = sys.argv[2] if len(sys.argv) > 2 else os.path.join(os.path.dirname(ROOT), os.path.basename(ROOT) + "-scratch")
MOD = os.path.join(ROOT, "mods/machinery_basis")


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
    write(f"{src}/math.rs", read(f"{src}/math.rs") + "\npub(crate) mod machinery_model_transform {\n" + read(f"{MOD}/model_transform.rs") + "\n}\n")
    model = patch_once(
        read(f"{src}/model.rs"),
        "        let mut textures = BTreeMap::new();\n        if let Some(raw) = obj.get(\"textures\")",
        "        let mut textures = machinery_model_transform_decode(raw, BTreeMap::new());\n        if let Some(raw) = obj.get(\"textures\")",
    )
    write(f"{src}/model.rs", model + "\n" + read(f"{MOD}/model_transform_decode.rs"))
    bake = patch_once(
        read(f"{src}/bake.rs"),
        "    output: &mut Vec<ModelQuad>,\n) -> Result<()> {\n    let render_type = model.render_type.as_deref().and_then(RenderType::by_name);\n    for element in model.elements() {",
        "    output: &mut Vec<ModelQuad>,\n) -> Result<()> {\n    if let core::ops::ControlFlow::Break(v) = machinery_model_transform_bake(model, transform, skip_flat, &mut *output) {\n        return v;\n    }\n    let render_type = model.render_type.as_deref().and_then(RenderType::by_name);\n    for element in model.elements() {",
    )
    write(f"{src}/bake.rs", bake + "\n" + read(f"{MOD}/model_transform_bake.rs"))
    os.makedirs(f"{SCRATCH}/crates/horizon-assets/tests")
    shutil.copy(f"{HERE}/testes/horizon_assets_transform.rs", f"{SCRATCH}/crates/horizon-assets/tests/transform.rs")
    env = dict(os.environ, CONTENT=f"{ROOT}/mods/mekanismgenerators/content/assets")
    sys.exit(subprocess.call(["cargo", "test", "--offline", "-p", "horizon-assets"], cwd=SCRATCH, env=env))


if __name__ == "__main__":
    main()
