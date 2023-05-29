use glam::Vec3;
use phf::phf_map;

static SLOPE_ANGLES: phf::Map<&'static str, i32> = phf_map! {
    "962.dat" => 45,
    "2341.dat" => 45,
    "2449.dat" => 45,
    "2875.dat" => 45,
    "2876.dat" => 40,
    "3037.dat" => 45,
    "3038.dat" => 45,
    "3039.dat" => 45,
    "3040.dat" => 45,
    "3041.dat" => 45,
    "3042.dat" => 45,
    "3043.dat" => 45,
    "3044.dat" => 45,
    "3045.dat" => 45,
    "3046.dat" => 45,
    "3048.dat" => 45,
    "3049.dat" => 45,
    "3135.dat" => 45,
    "3297.dat" => 45,
    "3298.dat" => 45,
    "3299.dat" => 45,
    "3300.dat" => 45,
    "3660.dat" => 45,
    "3665.dat" => 45,
    "3675.dat" => 45,
    "3676.dat" => 45,
    "3678b.dat" => 45,
    "3684.dat" => 45,
    "3685.dat" => 45,
    "3688.dat" => 45,
    "3747.dat" => 45,
    "4089.dat" => 45,
    "4161.dat" => 45,
    "4286.dat" => 45,
    "4287.dat" => 45,
    "4445.dat" => 45,
    "4460.dat" => 45,
    "4509.dat" => 45,
    "4854.dat" => 45,
    "4856.dat" => 45,
    "4857.dat" => 45,
    "4858.dat" => 45,
    "4861.dat" => 45,
    "4871.dat" => 45,
    "6069.dat" => 45,
    "6153.dat" => 45,
    "6227.dat" => 45,
    "6270.dat" => 45,
    "13269.dat" => 45,
    "13548.dat" => 45,
    "15571.dat" => 45,
    "18759.dat" => 45,
    "22390.dat" => 45,
    "22391.dat" => 45,
    "22889.dat" => 45,
    "28192.dat" => 45,
    "30180.dat" => 45,
    "30182.dat" => 45,
    "30183.dat" => 45,
    "30249.dat" => 45,
    "30283.dat" => 45,
    "30363.dat" => 45,
    "30373.dat" => 45,
    "30382.dat" => 45,
    "30390.dat" => 45,
    "30499.dat" => 45,
    "32083.dat" => 45,
    "43708.dat" => 45,
    "43710.dat" => 45,
    "43711.dat" => 45,
    "47759.dat" => 45,
    "52501.dat" => 45,
    "60219.dat" => 45,
    "60477.dat" => 45,
    "60481.dat" => 45,
    "63341.dat" => 45,
    "72454.dat" => 45,
    "92946.dat" => 45,
    "93348.dat" => 45,
    "95188.dat" => 45,
    "99301.dat" => 45,
    "303923.dat" => 45,
    "303926.dat" => 45,
    "304826.dat" => 45,
    "329826.dat" => 45,
    "374726.dat" => 45,
    "428621.dat" => 45,
    "4162628.dat" => 45,
    "4195004.dat" => 45,
};

pub fn is_slope_piece(name: &str) -> bool {
    // TODO: some parts have suffixes like a or b or p?
    SLOPE_ANGLES.contains_key(name)
}

pub fn is_grainy_slope(face: &[Vec3], is_slope: bool, is_stud: bool) -> bool {
    // Studs are always smooth regardless of their slopes.
    if is_slope && !is_stud {
        // Check if the vertical face angle is in the expected range.
        // This is the approach used by the previous ImportLDraw addon:
        // https://github.com/TobyLobster/ImportLDraw/blob/master/loadldraw/loadldraw.py
        let normal = (face[1] - face[0]).cross(face[2] - face[0]).normalize();
        let cosine = normal.y.clamp(-1.0, 1.0);
        let angle_to_ground = cosine.acos().to_degrees() - 90.0;
        // TODO: Set a per part angle threshold.
        (15.0..=75.0).contains(&angle_to_ground.abs())
    } else {
        false
    }
}
