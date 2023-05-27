use std::{collections::HashMap, path::Path};

pub struct LDrawColor {
    pub name: String,
    pub finish_name: String,
    pub rgba_linear: [f32; 4],
    pub speckle_rgba_linear: Option<[f32; 4]>,
}

pub fn load_color_table(ldraw_path: &str) -> HashMap<u32, LDrawColor> {
    let config_path = Path::new(ldraw_path).join("LDConfig.ldr");
    let cmds = weldr::parse_raw(&std::fs::read(config_path).unwrap()).unwrap();

    cmds.into_iter()
        .filter_map(|cmd| match cmd {
            weldr::Command::Colour(c) => {
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

fn rgba_linear(value: &weldr::Color, alpha: Option<u8>) -> [f32; 4] {
    [
        srgb_to_linear(value.red as f32 / 255.0),
        srgb_to_linear(value.green as f32 / 255.0),
        srgb_to_linear(value.blue as f32 / 255.0),
        alpha.unwrap_or(255) as f32 / 255.0,
    ]
}

fn speckle_rgba_linear(c: &weldr::ColourCmd) -> Option<[f32; 4]> {
    c.finish.as_ref().and_then(|f| match f {
        weldr::ColorFinish::Material(weldr::MaterialFinish::Speckle(speckle)) => {
            Some(rgba_linear(&speckle.value, speckle.alpha))
        }
        _ => None,
    })
}

fn finish_name(c: &weldr::ColourCmd) -> &str {
    match &c.finish {
        Some(finish) => match finish {
            weldr::ColorFinish::Chrome => "Chrome",
            weldr::ColorFinish::Pearlescent => "Pearlescent",
            weldr::ColorFinish::Rubber => "Rubber",
            weldr::ColorFinish::MatteMetallic => "MatteMetallic",
            weldr::ColorFinish::Metal => "Metal",
            weldr::ColorFinish::Material(material) => match material {
                weldr::MaterialFinish::Glitter(_) => "Glitter",
                weldr::MaterialFinish::Speckle(_) => "Speckle",
                weldr::MaterialFinish::Other(name) => name,
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
