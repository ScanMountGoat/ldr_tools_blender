use std::collections::HashMap;

pub struct LDrawColor {
    pub name: String,
    pub rgba_linear: [f32; 4],
    pub finish_name: String,
}

pub fn load_color_table() -> HashMap<u32, LDrawColor> {
    let config_path = r"C:\Users\Public\Documents\LDraw\LDConfig.ldr";
    let cmds = weldr::parse_raw(&std::fs::read(config_path).unwrap()).unwrap();

    cmds.into_iter()
        .filter_map(|cmd| match cmd {
            weldr::Command::Colour(c) => {
                let finish_name = finish_name(&c).to_string();

                // LDraw colors are in sRGB space.
                let color = LDrawColor {
                    name: c.name,
                    rgba_linear: [
                        srgb_to_linear(c.value.red as f32 / 255.0),
                        srgb_to_linear(c.value.green as f32 / 255.0),
                        srgb_to_linear(c.value.blue as f32 / 255.0),
                        c.alpha.unwrap_or(255) as f32 / 255.0,
                    ],
                    finish_name,
                };
                Some((c.code, color))
            }
            _ => None,
        })
        .collect()
}

fn finish_name(c: &weldr::ColourCmd) -> &str {
    // TODO: How to handle pearlescent colors?
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
