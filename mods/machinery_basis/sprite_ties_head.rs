// Roda no começo de `HudRenderer::animate_sprites`. Os sprites animados e interpolados que uma tela de mod desenha ganham a mistura do
// original: o `animate_sprite_interpolate.fsh` faz `mix(atual, proximo, millis / 1000)` em `float` e grava em 8 bits, e nos empates
// exatos (`... .5`) o desktop arredonda para baixo, enquanto o Horizon arredonda para cima (`+ 500`). Aqui o empate vai para baixo
// (`+ 499`, o mesmo resultado em todo outro valor). Depois de desenhar, `previous` recebe o estado, e o laço original pula o sprite.
// Quadros inteiros (`millis == 0`) e sprites que nenhuma tela de mod desenhou ficam com o laço original.
fn machinery_basis_sprite_ties(
    hud: &mut HudRenderer,
    uploads: &mut crate::gfx::upload::Uploads,
    encoder: &mut wgpu::CommandEncoder,
    tick: u64,
) {
    let HudRenderer {
        atlas_texture,
        sprite_animations,
        ..
    } = hud;
    let Some(texture) = atlas_texture.as_ref() else {
        return;
    };
    for (rect, animation, previous) in sprite_animations.iter_mut() {
        if !machinery_sprite_ties::contains(rect) {
            continue;
        }
        let blend = animation.blend(tick);
        let state = (blend.frame, blend.next, blend.millis);
        if blend.millis == 0 || state == *previous {
            continue;
        }
        let (w, h) = rect.texels((texture.width(), texture.height()));
        let current = animation.resized_pixels(blend.frame, w, h);
        let next = animation.resized_pixels(blend.next, w, h);
        let millis = blend.millis;
        let pixels: Vec<u8> = current
            .iter()
            .zip(next)
            .map(|(&a, b)| ((u32::from(a) * (1000 - millis) + u32::from(b) * millis + 499) / 1000) as u8)
            .collect();
        uploads.texture(
            encoder,
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: (rect.u0 * texture.width() as f32).round() as u32,
                    y: (rect.v0 * texture.height() as f32).round() as u32,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            w,
            h,
            &pixels,
        );
        *previous = state;
    }
}
