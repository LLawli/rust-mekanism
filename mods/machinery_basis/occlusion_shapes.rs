// A forma de oclusão de cada bloco de mod que declara `shapes`, em `occlusion.nbt` (decisão 22 do plano, H51). O vanilla esconde a face do vizinho pela forma de oclusão
// (`BlockState.getOcclusionShape`, que o `Block` padrão faz igual à de seleção: `getShape(EmptyBlockGetter, ZERO)`), e o mesher do Horizon lê a mesma coisa da tabela `occlusion.nbt`.
// `Plan::nbt` copia para um bloco novo a forma de oclusão do gabarito (o cubo cheio do `lodestone`), e o Horizon não deixa o mod dizer a dele: com `can_occlude` verdadeiro, um bloco de forma
// menor escondia a face do chão por baixo (a grama sob a máquina sumia), e com `occludes: false` o vizinho desenhava faces que o original esconde. Este HEAD cancelável de `tables` (que a chama
// de novo por dentro, com a marca de reentrada, para o corpo original escrever as tabelas todas: o RETURN não aceita o `?` do corpo) lê `occlusion.nbt` do overlay, troca o índice de forma de cada estado dos blocos que têm `shapes` pela forma declarada (a mesma codificação de `collision_shapes`)
// e a reescreve. Um bloco de `thickness` (cano e cabo, que o Horizon faz com `pipe_shape`) tem a forma de oclusão do centro, sem os braços, como `BlockTransmitter.getOcclusionShape`
// (`getCenter()`), e `can_occlude` verdadeiro em `render-definitions.json` (o original o dá aos transmissores; o gabarito `chorus_plant` é `noOcclusion`).
// Um bloco sem `shapes` nem `thickness` fica com a forma do gabarito.
fn machinery_occlusion_shapes(base: &Path, plan: &Plan<'_>, overlay: &Overlay<'_>) -> core::ops::ControlFlow<Result<()>> {
    std::thread_local! {
        static INSIDE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if INSIDE.with(|inside| inside.replace(true)) {
        return core::ops::ControlFlow::Continue(());
    }
    let written = tables(base, plan, overlay);
    INSIDE.with(|inside| inside.set(false));
    let rewrite = || -> Result<()> {
        let mut declared: Vec<(usize, Vec<VoxelShape>)> = Vec::new();
        let mut pipes: Vec<&str> = Vec::new();
        for block in &plan.blocks {
            if let Some(size) = block.decl.thickness {
                let center = horizon_world::collision::pipe_shape(size.into(), [false; 6]).map_err(|e| e.to_string())?;
                declared.push((block.rename.first as usize, vec![center; block.order.len()]));
                pipes.push(block.decl.id.as_str());
            } else if let Some(shapes) = block.shapes.as_ref() {
                declared.push((block.rename.first as usize, shapes.clone()));
            }
        }
        if declared.is_empty() {
            return Ok(());
        }
        let path = overlay.out.join("occlusion.nbt");
        let (mut named, _) = nbt::read_named(&fs::read(&path)?, Limits::tables()).map_err(|e| format!("occlusion.nbt: {e}"))?;
        {
            let Tag::Compound(table) = &mut named.tag else {
                return Err("occlusion table is not a compound".into());
            };
            let mut indices = Vec::new();
            let mut encoded: BTreeMap<Vec<[u64; 6]>, i32> = BTreeMap::new();
            let Some(Tag::List(entries)) = table.get_mut("shapes") else {
                return Err("occlusion table has no shapes".into());
            };
            for (first, shapes) in &declared {
                for (offset, shape) in shapes.iter().enumerate() {
                    let key: Vec<[u64; 6]> = shape
                        .boxes()
                        .iter()
                        .map(|bounds| {
                            [
                                bounds.min(Axis::X),
                                bounds.min(Axis::Y),
                                bounds.min(Axis::Z),
                                bounds.max(Axis::X),
                                bounds.max(Axis::Y),
                                bounds.max(Axis::Z),
                            ]
                            .map(f64::to_bits)
                        })
                        .collect();
                    let id = *encoded.entry(key).or_insert_with(|| {
                        entries.push(encode_shape(shape));
                        entries.len() as i32 - 1
                    });
                    indices.push((first + offset, id));
                }
            }
            let Some(Tag::IntArray(mapping)) = table.get_mut("cached_shapes") else {
                return Err("occlusion table has no mapping".into());
            };
            for (state, id) in indices {
                *mapping.get_mut(state).ok_or("occlusion mapping is shorter than the registry")? = id;
            }
        }
        overlay.write("occlusion.nbt", &nbt::write_named(&named, Limits::tables()).map_err(|e| format!("occlusion.nbt: {e}"))?)?;
        if !pipes.is_empty() {
            let render = overlay.out.join("render-definitions.json");
            let mut doc = read_json(&render)?;
            for id in pipes {
                if let Some(entry) = doc.get_mut(id) {
                    entry.set("can_occlude", J::Bool(true));
                }
            }
            overlay.write("render-definitions.json", &serde_json::to_vec(&doc)?)?;
        }
        Ok(())
    };
    core::ops::ControlFlow::Break(match written {
        Ok(()) => rewrite(),
        failed => failed,
    })
}
