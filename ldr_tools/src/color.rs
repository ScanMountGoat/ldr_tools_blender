use std::collections::HashMap;

pub struct LDrawColor {
    pub name: String,
    pub rgba_linear: [f32; 3],
    pub is_metallic: bool,
    pub is_transmissive: bool,
}

pub fn load_color_table() -> HashMap<u32, LDrawColor> {
    let config_path = r"C:\Users\Public\Documents\LDraw\LDCfgalt.ldr";
    let cmds = weldr::parse_raw(&std::fs::read(config_path).unwrap()).unwrap();

    // LDraw colors are in sRGB space.
    cmds.into_iter()
        .filter_map(|cmd| match cmd {
            weldr::Command::Colour(c) => {
                let is_metallic = is_metallic(&c);
                let is_transmissive = is_transmissive(&c);

                let color = LDrawColor {
                    name: c.name,
                    rgba_linear: [
                        srgb_to_linear(c.value.red as f32 / 255.0),
                        srgb_to_linear(c.value.green as f32 / 255.0),
                        srgb_to_linear(c.value.blue as f32 / 255.0),
                    ],
                    is_metallic,
                    is_transmissive,
                };
                Some((c.code, color))
            }
            _ => None,
        })
        .collect()
}

fn is_metallic(c: &weldr::ColourCmd) -> bool {
    // TODO: How to handle pearlescent colors?
    match &c.finish {
        Some(finish) => match finish {
            weldr::ColorFinish::Chrome => true,
            weldr::ColorFinish::MatteMetallic => true,
            weldr::ColorFinish::Metal => true,
            _ => false,
        },
        None => false,
    }
}

fn is_transmissive(c: &weldr::ColourCmd) -> bool {
    // TODO: Is it worth using the builtin alpha values?
    // These probably won't work well for PBR renders.
    match &c.alpha {
        Some(alpha) => *alpha < 255u8,
        None => false,
    }
}

fn srgb_to_linear(srgb: f32) -> f32 {
    if srgb <= 0.04045 {
        srgb / 12.92
    } else {
        ((srgb + 0.055) / 1.055).powf(2.4)
    }
}
