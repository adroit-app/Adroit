//! How the window draws the icons of `adroit_core::icons`.

use dioxus::prelude::*;

pub use adroit_core::icons::*;

/// How thick every icon's lines are drawn, in the 24-unit grid Tabler draws
/// in.
pub const STROKE: &str = "1.75";

/// `icon` drawn `px` pixels square as RGBA, black on clear: the shape alone,
/// for macOS to colour as its menus are.
pub fn rgba(icon: Icon, px: u32) -> Option<Vec<u8>> {
    use resvg::{tiny_skia, usvg};
    let svg = icon
        .svg()
        .replace("stroke-width=\"2\"", &format!("stroke-width=\"{STROKE}\""));
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(px, px)?;
    let scale = px as f32 / tree.size().width();
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(pixmap.take())
}

/// An icon drawn in the text's colour, `size` pixels square.
#[component]
pub fn IconView(icon: Icon, #[props(default = 14)] size: u32) -> Element {
    rsx! {
        span {
            class: "icon icon-{icon.name()}",
            style: "--icon:{size}px",
            "aria-hidden": "true",
            dangerous_inner_html: icon.svg(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_icon_is_drawn_as_many_pixels_square_as_asked() {
        let rgba = rgba(Icon::Settings, 36).unwrap();
        assert_eq!(rgba.len(), 36 * 36 * 4);
    }

    #[test]
    fn a_drawn_icon_is_its_shape_in_black_on_clear() {
        let rgba = rgba(Icon::Plus, 36).unwrap();
        let pixels: Vec<&[u8]> = rgba.chunks(4).collect();
        assert!(pixels.iter().any(|p| p[3] > 0), "something is drawn");
        assert!(pixels.iter().any(|p| p[3] == 0), "around it is clear");
        assert!(pixels.iter().all(|p| p[..3] == [0, 0, 0]));
        // The plus crosses at the centre and leaves the corners clear.
        assert!(pixels[18 * 36 + 18][3] > 0);
        assert_eq!(pixels[0][3], 0);
    }
}
