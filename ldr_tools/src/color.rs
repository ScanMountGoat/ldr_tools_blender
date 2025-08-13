use std::{collections::HashMap, path::Path};

pub struct LDrawColor {
    pub name: String,
    pub finish_name: String,
    pub rgba_linear: [f32; 4],
    pub speckle_rgba_linear: Option<[f32; 4]>,
}

pub fn load_color_table(ldraw_path: &str) -> HashMap<u32, LDrawColor> {
    let config_path = Path::new(ldraw_path).join("LDConfig.ldr");
    let cmds = crate::ldraw::parse_commands(&std::fs::read(config_path).unwrap());

    cmds.into_iter()
        .filter_map(|cmd| match cmd {
            crate::ldraw::Command::Colour(c) => {
                // LDraw colors are in sRGB space.
                let rgba_linear = rgba_linear(&c.value, c.alpha);
                let speckle_rgba_linear = speckle_rgba_linear(&c);
                let finish_name = finish_name(&c).to_string();
                let color = LDrawColor {
                    name: c.name,
                    rgba_linear,
                    speckle_rgba_linear,
                    finish_name,
                };
                Some((c.code, color))
            }
            _ => None,
        })
        .collect()
}

fn rgba_linear(value: &crate::ldraw::Color, alpha: Option<u8>) -> [f32; 4] {
    [
        srgb_to_linear(value.red as f32 / 255.0),
        srgb_to_linear(value.green as f32 / 255.0),
        srgb_to_linear(value.blue as f32 / 255.0),
        alpha.unwrap_or(255) as f32 / 255.0,
    ]
}

fn speckle_rgba_linear(c: &crate::ldraw::ColourCmd) -> Option<[f32; 4]> {
    c.finish.as_ref().and_then(|f| match f {
        crate::ldraw::ColorFinish::Material(crate::ldraw::MaterialFinish::Speckle(speckle)) => {
            Some(rgba_linear(&speckle.value, speckle.alpha))
        }
        _ => None,
    })
}

fn finish_name(c: &crate::ldraw::ColourCmd) -> &str {
    match &c.finish {
        Some(finish) => match finish {
            crate::ldraw::ColorFinish::Chrome => "Chrome",
            crate::ldraw::ColorFinish::Pearlescent => "Pearlescent",
            crate::ldraw::ColorFinish::Rubber => "Rubber",
            crate::ldraw::ColorFinish::MatteMetallic => "MatteMetallic",
            crate::ldraw::ColorFinish::Metal => "Metal",
            crate::ldraw::ColorFinish::Material(material) => match material {
                crate::ldraw::MaterialFinish::Glitter(_) => "Glitter",
                crate::ldraw::MaterialFinish::Speckle(_) => "Speckle",
                crate::ldraw::MaterialFinish::Other(name) => name,
            },
        },
        None => "",
    }
}

fn srgb_to_linear(srgb: f32) -> f32 {
    if srgb <= 0.04045 {
        srgb / 12.92
    } else {
        ((srgb + 0.055) / 1.055).powf(2.4)
    }
}
