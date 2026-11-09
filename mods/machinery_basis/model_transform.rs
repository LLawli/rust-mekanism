// Módulo de `math.rs` do `horizon-assets`: o `transform` de modelo do NeoForge (a chave `transform` na raiz de um modelo e em cada filho de um `neoforge:composite`), que o
// Horizon não lê. Este arquivo é só a conta, sem tipo do Horizon (usa `std` e `serde_json`), para os testes compilarem-no como está; o encaixe nos tipos do Horizon fica
// nas duas injeções `model_transform_decode` (`CuboidModel::decode`) e `model_transform_bake` (`bake_into`).
//
// Semântica do NeoForge, `TransformationHelper.Deserializer` e `UnbakedElementsHelper.composeRootTransformIntoModelState`:
// - o JSON vira uma `Transformation` (`matrix`, ou `translation`, `rotation`/`left_rotation`, `scale`, `right_rotation`/`post-rotation`, `origin`; a origem padrão é o canto
//   oposto, `(1, 1, 1)`) e `applyOrigin(origem)` a leva para a origem pedida;
// - ao cozinhar, a transformação de raiz vai a `applyOrigin(-0,5; -0,5; -0,5)` e se junta, à direita, ao `ModelState` do pai (a rotação do blockstate): `pai.compose(raiz)`.
//   O `FaceBakery` e o `ObjGeometry` aplicam o resultado em volta do centro do bloco, `v - 0,5`, matriz, `+ 0,5`;
// - a propriedade é por modelo (o filho mais perto da folha vence, `fillRootTransformProperty`), e o filho de um `composite` junta a sua à do `composite`.
//
// O Horizon aplica a rotação do blockstate (R) já nos vértices, então aqui a matriz do modelo (M) vai depois, conjugada: `R M R^-1` aplicada ao vértice já girado dá `R M` no
// vértice original, que é o `pai.compose(raiz)` do NeoForge. A conta de M é a do JOML na mesma ordem de operações; com R identidade a conta de cada vértice é a mesma do original.
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU64, Ordering};

/// A chave do espaço de textura que carrega a transformação do modelo até o cozimento (o `CuboidModel` não tem campo para ela). O espaço vem do `textures` do JSON, que o
/// Horizon herda do pai para o filho e deixa o modelo mais perto da folha vencer, como o NeoForge faz com a propriedade. Nenhuma face o usa.
pub const SLOT: &str = "\u{1}neoforge_transform";
/// O começo do "sprite" do espaço: nenhum arquivo existe com esse espaço de nomes, então a pré-carga do Horizon o ignora.
const PREFIX: &str = "neoforge_transform:";

/// Uma matriz afim 3 por 4 por linhas: `[linha][coluna]`, a coluna 3 é a translação.
pub type Rows = [[f32; 4]; 3];

pub const IDENTITY: Rows = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// O modelo de um `bake_into` em andamento que a injeção já tratou.
    static BAKING: Cell<usize> = const { Cell::new(0) };
    /// Os ids das transformações de raiz em uso, de fora para dentro: um filho que só herda a do `composite` não a aplica de novo.
    static ACTIVE: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

// ---------------------------------------------------------------------------
// Conta de matrizes (JOML)
// ---------------------------------------------------------------------------

fn is_identity(m: &Rows) -> bool {
    *m == IDENTITY
}

/// `Matrix4f.mul` de duas matrizes afins: `a * b`, na ordem de somas do JOML (`mulAffine`).
fn mul(a: &Rows, b: &Rows) -> Rows {
    if is_identity(a) {
        return *b;
    }
    if is_identity(b) {
        return *a;
    }
    std::array::from_fn(|r| {
        std::array::from_fn(|c| {
            a[r][0] * b[0][c]
                + (a[r][1] * b[1][c]
                    + if c == 3 {
                        a[r][2] * b[2][c] + a[r][3]
                    } else {
                        a[r][2] * b[2][c]
                    })
        })
    })
}

fn translation(t: [f32; 3]) -> Rows {
    [[1.0, 0.0, 0.0, t[0]], [0.0, 1.0, 0.0, t[1]], [0.0, 0.0, 1.0, t[2]]]
}

/// A parte de rotação de `Matrix4f.rotate(Quaternionfc)`, sem normalizar o quaternion.
fn rotation(q: [f32; 4]) -> Rows {
    let [x, y, z, w] = q;
    let (w2, x2, y2, z2) = (w * w, x * x, y * y, z * z);
    let (zw, xy, xz, yw, yz, xw) = (z * w, x * y, x * z, y * w, y * z, x * w);
    let (dzw, dxy, dxz, dyw, dyz, dxw) = (zw + zw, xy + xy, xz + xz, yw + yw, yz + yz, xw + xw);
    [
        [w2 + x2 - z2 - y2, -dzw + dxy, dyw + dxz, 0.0],
        [dxy + dzw, y2 - z2 + w2 - x2, dyz - dxw, 0.0],
        [dxz - dyw, dyz + dxw, z2 - y2 - x2 + w2, 0.0],
    ]
}

fn scaling(s: [f32; 3]) -> Rows {
    [[s[0], 0.0, 0.0, 0.0], [0.0, s[1], 0.0, 0.0], [0.0, 0.0, s[2], 0.0]]
}

/// `Transformation.applyOrigin`: a identidade continua identidade; senão `T(origem) * M * T(-origem)`.
pub fn apply_origin(m: &Rows, origin: [f32; 3]) -> Rows {
    if is_identity(m) {
        return IDENTITY;
    }
    let moved = mul(&translation(origin), m);
    mul(&moved, &translation(origin.map(|c| -c)))
}

/// O `sin` do JOML (`Math.sin(float)` sem fast math).
fn joml_sin(rad: f32) -> f32 {
    f64::from(rad).sin() as f32
}

/// `Math.cosFromSin(float, float)` sem fast math: o módulo vem do seno e o sinal do quadrante do ângulo.
fn cos_from_sin(sin: f32, angle: f32) -> f32 {
    let cos = (1.0 - sin * sin).sqrt();
    let tau = (std::f64::consts::PI * 2.0) as f32;
    let a = angle + std::f32::consts::FRAC_PI_2;
    let mut b = a - (a / tau) as i32 as f32 * tau;
    if b < 0.0 {
        b += tau;
    }
    if b >= std::f32::consts::PI { -cos } else { cos }
}

fn half_sin_cos(angle: f32) -> (f32, f32) {
    let half = angle * 0.5;
    let sin = joml_sin(half);
    (sin, cos_from_sin(sin, half))
}

/// `Quaternionf.rotationXYZ`, ângulos em radianos. Quaternion como `[x, y, z, w]`.
fn quaternion_xyz(x: f32, y: f32, z: f32) -> [f32; 4] {
    let ((sx, cx), (sy, cy), (sz, cz)) = (half_sin_cos(x), half_sin_cos(y), half_sin_cos(z));
    let (cx_cy, sx_cy, cx_sy, sx_sy) = (cx * cy, sx * cy, cx * sy, sx * sy);
    [sx_cy * cz + cx_sy * sz, cx_sy * cz - sx_cy * sz, cx_cy * sz + sx_sy * cz, cx_cy * cz - sx_sy * sz]
}

/// `Quaternionf.mul(Quaternionf)`: `a * b`.
fn quaternion_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

/// `(float) Math.PI / 180`, o `Mth.DEG_TO_RAD`.
const DEG_TO_RAD: f32 = std::f32::consts::PI / 180.0;

// ---------------------------------------------------------------------------
// O JSON (`TransformationHelper.Deserializer`)
// ---------------------------------------------------------------------------

fn number(value: &Value, what: &str) -> Result<f32, String> {
    match value {
        Value::Number(n) => n.to_string().parse::<f32>().map_err(|_| format!("{what}: not a float")),
        other => Err(format!("{what}: expected number, got: {other}")),
    }
}

fn float_array<const N: usize>(value: &Value, what: &str) -> Result<[f32; N], String> {
    let Value::Array(list) = value else {
        return Err(format!("{what}: expected an array, got: {value}"));
    };
    if list.len() != N {
        return Err(format!("{what}: expected an array of length {N}, got: {}", list.len()));
    }
    let mut out = [0.0; N];
    for (slot, element) in out.iter_mut().zip(list) {
        *slot = number(element, &format!("{what} element"))?;
    }
    Ok(out)
}

fn matrix(value: &Value) -> Result<Rows, String> {
    let Value::Array(rows) = value else {
        return Err(format!("Matrix: expected an array, got: {value}"));
    };
    if rows.len() != 3 {
        return Err(format!("Matrix: expected an array of length 3, got: {}", rows.len()));
    }
    let mut out = IDENTITY;
    for (r, row) in rows.iter().enumerate() {
        out[r] = float_array::<4>(row, "Matrix row")?;
    }
    Ok(out)
}

fn axis_rotation(value: &Value) -> Result<[f32; 4], String> {
    let Value::Object(map) = value else {
        return Err(format!("Axis rotation: object expected, got: {value}"));
    };
    if map.len() != 1 {
        return Err(format!("Axis rotation: expected single axis object, got: {value}"));
    }
    let (axis, degrees) = map.iter().next().ok_or("Axis rotation: empty")?;
    let angle = number(degrees, "Axis rotation value")? * DEG_TO_RAD;
    let (sin, cos) = half_sin_cos(angle);
    match axis.as_str() {
        "x" => Ok([sin, 0.0, 0.0, cos]),
        "y" => Ok([0.0, sin, 0.0, cos]),
        "z" => Ok([0.0, 0.0, sin, cos]),
        other => Err(format!("Axis rotation: expected single axis key, got: {other}")),
    }
}

fn rotation_quaternion(value: &Value) -> Result<[f32; 4], String> {
    match value {
        Value::Array(list) => {
            let first = list.first().ok_or("Rotation: empty array")?;
            if first.is_object() {
                let mut out = [0.0, 0.0, 0.0, 1.0];
                for axis in list {
                    out = quaternion_mul(out, axis_rotation(axis)?);
                }
                Ok(out)
            } else if list.len() == 3 {
                let [x, y, z] = float_array::<3>(value, "Rotation")?;
                Ok(quaternion_xyz(x * DEG_TO_RAD, y * DEG_TO_RAD, z * DEG_TO_RAD))
            } else {
                float_array::<4>(value, "Rotation")
            }
        }
        Value::Object(_) => axis_rotation(value),
        other => Err(format!("Rotation: expected array or object, got: {other}")),
    }
}

fn origin(map: &serde_json::Map<String, Value>) -> Result<[f32; 3], String> {
    match map.get("origin") {
        Some(array @ Value::Array(_)) => float_array::<3>(array, "Origin"),
        Some(Value::String(name)) => match name.as_str() {
            "center" => Ok([0.5, 0.5, 0.5]),
            "corner" => Ok([0.0; 3]),
            "opposing-corner" => Ok([1.0; 3]),
            _ => Err("Origin: expected one of 'center', 'corner', 'opposing-corner'".to_owned()),
        },
        _ => Err("Origin: expected an array or one of 'center', 'corner', 'opposing-corner'".to_owned()),
    }
}

/// O valor da chave `transform` de um modelo, como `TransformationHelper.Deserializer.deserialize`.
pub fn parse(value: &Value) -> Result<Rows, String> {
    let map = match value {
        Value::String(text) if text == "identity" => return Ok(IDENTITY),
        Value::String(text) => return Err(format!("TRSR: unknown default string: {text}")),
        Value::Array(_) => return matrix(value),
        Value::Object(map) => map,
        other => return Err(format!("TRSR: expected array or object, got: {other}")),
    };
    if let Some(rows) = map.get("matrix") {
        let out = matrix(rows)?;
        if map.len() > 1 {
            return Err("TRSR: can't combine matrix and other keys".to_owned());
        }
        return Ok(out);
    }
    let mut keys: std::collections::BTreeSet<&str> = map.keys().map(String::as_str).collect();
    let mut take = |name: &'static str| keys.remove(name).then(|| &map[name]);
    let translation_value = take("translation").map(|v| float_array::<3>(v, "Translation")).transpose()?;
    let left = match take("rotation") {
        Some(v) => Some(rotation_quaternion(v)?),
        None => take("left_rotation").map(rotation_quaternion).transpose()?,
    };
    let scale_value = take("scale")
        .map(|v| match v {
            Value::Array(_) => float_array::<3>(v, "Scale"),
            other => number(other, "TRSR scale").map(|s| [s, s, s]),
        })
        .transpose()?;
    let right = match take("right_rotation") {
        Some(v) => Some(rotation_quaternion(v)?),
        None => take("post-rotation").map(rotation_quaternion).transpose()?,
    };
    let origin_value = if take("origin").is_some() { origin(map)? } else { [1.0; 3] };
    if !keys.is_empty() {
        return Err(format!(
            "TRSR: can either have single 'matrix' key, or a combination of 'translation', 'rotation' OR 'left_rotation', 'scale', 'post-rotation' (legacy) OR 'right_rotation', 'origin'. Found: {}",
            keys.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    // `new Transformation(translation, left, scale, right)`: `Matrix4f().translation(t).rotate(left).scale(s).rotate(right)`, pulando o que falta.
    let mut out = IDENTITY;
    if let Some(t) = translation_value {
        out = translation(t);
    }
    if let Some(q) = left {
        out = mul(&out, &rotation(q));
    }
    if let Some(s) = scale_value {
        out = mul(&out, &scaling(s));
    }
    if let Some(q) = right {
        out = mul(&out, &rotation(q));
    }
    Ok(apply_origin(&out, origin_value))
}

// ---------------------------------------------------------------------------
// Do JSON do modelo ao "sprite" do espaço, e de volta
// ---------------------------------------------------------------------------

/// O texto do JSON de um modelo cita `transform`? Barato, para a injeção só ler o JSON de quem pode ter.
pub fn mentions(text: &str) -> bool {
    text.contains("\"transform\"")
}

fn encode(id: u64, m: &Rows) -> String {
    let mut out = format!("{PREFIX}{id:x}");
    for row in m {
        for value in row {
            out.push_str(&format!("_{:08x}", value.to_bits()));
        }
    }
    out
}

/// O id e a matriz que `encode` guardou no "sprite" do espaço.
pub fn decode_sprite(sprite: &str) -> Option<(u64, Rows)> {
    let mut parts = sprite.strip_prefix(PREFIX)?.split('_');
    let id = u64::from_str_radix(parts.next()?, 16).ok()?;
    let mut out = IDENTITY;
    for row in &mut out {
        for value in row {
            *value = f32::from_bits(u32::from_str_radix(parts.next()?, 16).ok()?);
        }
    }
    parts.next().is_none().then_some((id, out))
}

/// O "sprite" do espaço `SLOT` do modelo cujo JSON é `text`, ou `None` se ele não tem `transform` na raiz. Erro: o `transform` que o NeoForge recusa (o modelo não carrega).
pub fn slot_sprite(text: &str) -> Result<Option<String>, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let Some(transform) = value.as_object().and_then(|map| map.get("transform")) else {
        return Ok(None);
    };
    let m = parse(transform)?;
    Ok(Some(encode(NEXT_ID.fetch_add(1, Ordering::Relaxed), &m)))
}

// ---------------------------------------------------------------------------
// O cozimento
// ---------------------------------------------------------------------------

/// A injeção de `bake_into` já tratou este modelo? Limpa a marca.
pub fn bake_reentered(model: usize) -> bool {
    BAKING.with(|mark| {
        let same = mark.get() == model;
        if same {
            mark.set(0);
        }
        same
    })
}

pub fn bake_mark(model: usize) {
    BAKING.with(|mark| mark.set(model));
}

/// A transformação de raiz do modelo que o NeoForge junta ao `ModelState`: a do JSON com `applyOrigin(-0,5)`. `None` se o espaço não tem transformação, se ela é a identidade
/// (`composeRootTransformIntoModelState` devolve o estado como está) ou se é a que o `composite` de fora já aplicou (o filho só a herdou). Quem recebe `Some` chama `leave`.
pub fn enter(sprite: &str) -> Option<Rows> {
    let (id, m) = decode_sprite(sprite)?;
    if is_identity(&m) || ACTIVE.with(|stack| stack.borrow().last() == Some(&id)) {
        return None;
    }
    ACTIVE.with(|stack| stack.borrow_mut().push(id));
    Some(apply_origin(&m, [-0.5; 3]))
}

pub fn leave() {
    ACTIVE.with(|stack| {
        stack.borrow_mut().pop();
    });
}

/// A matriz a aplicar aos vértices que o Horizon já girou pela rotação `r` do blockstate (`BlockModelTransform::matrix_rows`): `r * m * r^-1` em torno do centro. Com `r`
/// identidade é `m`.
pub fn placement(r: &Rows, m: &Rows) -> Rows {
    if is_identity(r) {
        return *m;
    }
    let mut inverse = IDENTITY;
    for (i, row) in inverse.iter_mut().enumerate() {
        for (j, value) in row.iter_mut().take(3).enumerate() {
            *value = r[j][i];
        }
    }
    mul(&mul(r, m), &inverse)
}

/// `rotateVertexBy(vertex, BLOCK_MIDDLE, matriz)`: `v - 0,5`, a matriz, `+ 0,5`.
pub fn position(g: &Rows, v: [f32; 3]) -> [f32; 3] {
    let c = v.map(|a| a - 0.5);
    std::array::from_fn(|r| (g[r][0] * c[0] + (g[r][1] * c[1] + (g[r][2] * c[2] + g[r][3]))) + 0.5)
}

/// A matriz só move (a parte 3 por 3 é a identidade): as faces não mudam de direção.
pub fn moves_only(g: &Rows) -> bool {
    (0..3).all(|r| (0..3).all(|c| g[r][c] == if r == c { 1.0 } else { 0.0 }))
}

/// Os vetores unitários das seis direções, na ordem do Horizon e do Java (`Direction.values()`): baixo, cima, norte, sul, oeste, leste.
const STEPS: [[f32; 3]; 6] = [[0.0, -1.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0], [0.0, 0.0, 1.0], [-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]];

/// `Direction.getApproximateNearest(x, y, z)`: o índice da direção de maior produto escalar (a primeira em empate; norte se nenhum passa de zero).
pub fn nearest(v: [f32; 3]) -> usize {
    let mut best = f32::MIN_POSITIVE;
    let mut result = 2;
    for (index, step) in STEPS.iter().enumerate() {
        let dot = v[0] * step[0] + v[1] * step[1] + v[2] * step[2];
        if dot > best {
            best = dot;
            result = index;
        }
    }
    result
}

/// `Direction.rotate(matriz, direção)`: o vetor da direção pela parte 3 por 3 da matriz (w = 0), e a direção mais perto.
pub fn rotate_direction(g: &Rows, direction: usize) -> usize {
    let n = STEPS[direction];
    nearest(std::array::from_fn(|r| g[r][0] * n[0] + g[r][1] * n[1] + g[r][2] * n[2]))
}
