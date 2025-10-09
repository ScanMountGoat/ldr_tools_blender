use std::{collections::HashMap, path::Path};

#[derive(Debug, PartialEq, Clone)]
pub struct LDrawColor {
    pub name: String,
    pub finish_name: String,
    pub rgba_linear: [f32; 4],
    pub speckle_rgba_linear: Option<[f32; 4]>,
}

// TODO: Avoid unwrap.
pub fn load_color_table(ldraw_path: &str) -> HashMap<u32, LDrawColor> {
    // TODO: Is it better to combine both Studio and LDraw color information?
    let color_definition_path = Path::new(ldraw_path)
        .parent()
        .unwrap()
        .join("data")
        .join("CustomColorDefinition.txt");

    load_studio_color_table(color_definition_path)
        .or_else(|| {
            let config_path = Path::new(ldraw_path).join("LDConfig.ldr");
            load_ldraw_color_table(config_path)
        })
        .unwrap_or_default()
}

// TODO: Avoid unwrap and log errors.
pub fn load_ldraw_color_table<P: AsRef<Path>>(path: P) -> Option<HashMap<u32, LDrawColor>> {
    let bytes = std::fs::read(path).ok()?;
    let cmds = crate::ldraw::parse_commands(&bytes);

    let colors = cmds
        .into_iter()
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
        .collect();
    Some(colors)
}

// TODO: Avoid unwrap and log errors.
pub fn load_studio_color_table<P: AsRef<Path>>(path: P) -> Option<HashMap<u32, LDrawColor>> {
    let text = std::fs::read_to_string(path).ok()?;
    // Studio uses a format similar to csv but with tabs as the separator.
    let mut lines = text.lines();
    let header_names: Vec<_> = lines.next().unwrap().split("\t").collect();
    let ldraw_color_code_index = header_names
        .iter()
        .position(|n| *n == "LDraw Color Code")
        .unwrap();
    let rgb_index = header_names.iter().position(|n| *n == "RGB value").unwrap();
    let alpha = header_names.iter().position(|n| *n == "Alpha").unwrap();
    let studio_name = header_names
        .iter()
        .position(|n| *n == "Studio Color Name")
        .unwrap();

    let mut colors = HashMap::new();
    for line in lines {
        let parts: Vec<_> = line.split("\t").collect();

        let ldraw_color_code: u32 = parts[ldraw_color_code_index].parse().unwrap();

        let rgb = parts[rgb_index].trim_start_matches('#');
        let r = u32::from_str_radix(&rgb[..2], 16).unwrap();
        let g = u32::from_str_radix(&rgb[2..4], 16).unwrap();
        let b = u32::from_str_radix(&rgb[4..6], 16).unwrap();

        let rgba_linear = [
            srgb_to_linear(r as f32 / 255.0),
            srgb_to_linear(g as f32 / 255.0),
            srgb_to_linear(b as f32 / 255.0),
            parts[alpha].parse().unwrap(),
        ];

        // TODO: estimate the finish name.
        // TODO: Does studio store the speckle color?
        let color = LDrawColor {
            name: parts[studio_name].to_string(),
            finish_name: String::new(),
            rgba_linear,
            speckle_rgba_linear: None,
        };
        colors.insert(ldraw_color_code, color);
    }

    Some(colors)
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
