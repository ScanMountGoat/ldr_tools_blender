use std::collections::HashMap;

pub struct LDrawColor {
    pub name: String,
    pub value: [f32; 3],
}

pub fn load_color_table() -> HashMap<u32, LDrawColor> {
    let config_path = r"C:\Users\Public\Documents\LDraw\LDCfgalt.ldr";
    let cmds = weldr::parse_raw(&std::fs::read(config_path).unwrap()).unwrap();

    // LDraw colors are in sRGB space.
    cmds.into_iter()
        .filter_map(|cmd| match cmd {
            weldr::Command::Colour(c) => {
                let color = LDrawColor {
                    name: c.name,
                    value: [
                        srgb_to_linear(c.value.red as f32 / 255.0),
                        srgb_to_linear(c.value.green as f32 / 255.0),
                        srgb_to_linear(c.value.blue as f32 / 255.0),
                    ],
                };
                Some((c.code, color))
            }
            _ => None,
        })
        .collect()
}

fn srgb_to_linear(srgb: f32) -> f32 {
    if srgb <= 0.04045 {
        srgb / 12.92
    } else {
        ((srgb + 0.055) / 1.055).powf(2.4)
    }
}
