// Módulo de `obj.rs` do `horizon-assets`: o que um `.mtl` diz além do `map_Kd`, que o Horizon não lê. O NeoForge (`ObjMaterialLibrary`, `ObjGeometry.makeQuad`) guarda por material a
// cor difusa `Kd` (um `Vector4f`, de 1, 1, 1, 1 por padrão) e o `neoforge_TintIndex` (-1 por padrão, `forge_TintIndex` também vale); cada vértice do quad leva a cor `Kd`
// (`quadBaker.setColor(float, float, float, float)`, que trunca `(int)(x * 255.0F)` e junta com `ARGB.color`, que mascara cada canal em 8 bits), e o quad leva o índice de tinta.
// Os materiais do cabo têm `Kd 0.8 0.8 0.8`: sem ler isso tudo sai 20% mais claro; e os do vidro do transportador e do conteúdo têm `neoforge_TintIndex 0`.
//
// Este arquivo é só a conta, sem tipo do Horizon (usa `std`), para os testes compilarem-no como está; o encaixe nos tipos do Horizon fica nas duas injeções `obj_material_parse`
// (o fim de `parse_mtl`) e `obj_material_quads` (o começo de `bake_obj`, que o chama de novo e depois mexe nos quads que ele acrescentou). `parse_mtl` devolve um mapa de material para textura que não tem lugar para essas duas coisas: ela as
// guarda no mesmo mapa, sob uma chave `PREFIX` + nome do material que nenhuma face usa (uma face procura o próprio nome), e `bake_obj` as lê de volta.

use std::cell::Cell;

thread_local! {
    /// O `ObjModel` de um `bake_obj` em andamento que a injeção já tratou (o endereço dele; 0 se nenhum).
    static BAKING: Cell<usize> = const { Cell::new(0) };
}

/// A injeção de `bake_obj` já tratou este OBJ? Limpa a marca. A injeção é um HEAD cancelável (o corpo de `bake_obj` tem `?`, e o RETURN não o aceita): chama `bake_obj` de novo por dentro,
/// com a marca posta, e a segunda passagem segue para o corpo original.
pub fn bake_reentered(obj: usize) -> bool {
    BAKING.with(|mark| {
        let same = mark.get() == obj;
        if same {
            mark.set(0);
        }
        same
    })
}

pub fn bake_mark(obj: usize) {
    BAKING.with(|mark| mark.set(obj));
}

/// O começo da chave de um material extra. O `\u{1}` não está em nome de material nenhum (o `parse_mtl` do Horizon lê o nome como a primeira palavra da linha `newmtl`).
pub const PREFIX: &str = "\u{1}mtl:";

/// `Kd 1 1 1` (ou 1 1 1 1), a cor padrão do material.
const WHITE: i32 = -1;

/// Sem `Kd` o quad fica como estava (`color` vazio, que o Horizon lê como branco), e sem `neoforge_TintIndex` o índice é -1.
pub const NO_TINT: i32 = -1;

/// `Kd` e `neoforge_TintIndex` de um material: a cor ARGB que os vértices levam (`None` quando o `.mtl` não tem `Kd`) e o índice de tinta.
pub type Properties = (Option<i32>, i32);

/// `ObjGeometry.parseVector4`: de 1 a 4 números; os que faltam valem 0 e o quarto, 1.
fn vector4(words: &[&str]) -> [f32; 4] {
    let number = |i: usize| {
        words
            .get(i)
            .and_then(|w| w.parse::<f32>().ok())
            .unwrap_or(0.0)
    };
    match words.len() {
        0 => [0.0; 4],
        1 => [number(0), 0.0, 0.0, 1.0],
        2 => [number(0), number(1), 0.0, 1.0],
        3 => [number(0), number(1), number(2), 1.0],
        _ => [number(0), number(1), number(2), number(3)],
    }
}

/// `VertexConsumer.setColor(float r, g, b, a)` seguido de `ARGB.color(alpha, red, green, blue)`: cada canal é `(int)(x * 255.0F)` e entra mascarado em 8 bits.
pub fn color(rgba: [f32; 4]) -> i32 {
    let channel = |x: f32| (x * 255.0) as i32 & 0xFF;
    channel(rgba[3]) << 24 | channel(rgba[0]) << 16 | channel(rgba[1]) << 8 | channel(rgba[2])
}

/// Os materiais do `.mtl` com `Kd` ou `neoforge_TintIndex`, na ordem em que aparecem: o nome e as duas propriedades (a última linha de cada vence, como no `ObjMaterialLibrary`).
pub fn properties(text: &str) -> Vec<(String, Properties)> {
    let mut found: Vec<(String, Properties)> = Vec::new();
    let mut current: Option<usize> = None;
    for line in text.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some((keyword, rest)) = words.split_first() else {
            continue;
        };
        match *keyword {
            "newmtl" => {
                current = rest.first().map(|name| {
                    match found.iter().position(|(known, _)| known == name) {
                        // Um `newmtl` repetido recomeça o material (`materials.put(name, new Material(name))`).
                        Some(at) => {
                            found[at].1 = (None, NO_TINT);
                            at
                        }
                        None => {
                            found.push(((*name).to_owned(), (None, NO_TINT)));
                            found.len() - 1
                        }
                    }
                });
            }
            "Kd" => {
                if let Some(at) = current {
                    // Branco multiplicado é a cor que o quad já tem: fica como o Horizon o assa (`color` vazio), e o modelo sem `Kd` útil não muda.
                    found[at].1.0 = Some(color(vector4(rest))).filter(|argb| *argb != WHITE);
                }
            }
            "forge_TintIndex" | "neoforge_TintIndex" => {
                if let (Some(at), Some(index)) =
                    (current, rest.first().and_then(|w| w.parse().ok()))
                {
                    found[at].1.1 = index;
                }
            }
            _ => {}
        }
    }
    found.retain(|(_, (kd, tint))| kd.is_some() || *tint != NO_TINT);
    found
}

/// A chave do material extra no mapa de `parse_mtl`.
pub fn key(material: &str) -> String {
    format!("{PREFIX}{material}")
}

/// A propriedade escrita no valor do material extra (o texto de um `MtlTexture::Slot`): `Kd` em hexadecimal ARGB (vazio sem `Kd`) e o índice, separados por `|`.
pub fn encode((kd, tint): Properties) -> String {
    match kd {
        Some(argb) => format!("{:08x}|{tint}", argb as u32),
        None => format!("|{tint}"),
    }
}

/// O contrário de [`encode`]; `None` para um texto que não é de um material extra.
pub fn decode(text: &str) -> Option<Properties> {
    let (kd, tint) = text.split_once('|')?;
    let kd = if kd.is_empty() {
        None
    } else {
        Some(u32::from_str_radix(kd, 16).ok()? as i32)
    };
    Some((kd, tint.parse().ok()?))
}
