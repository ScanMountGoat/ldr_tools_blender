# Overrides more suitable for PBR rendering for certain LDraw colors.
# Most of these colors are taken from Peeron: http://www.peeron.com/inv/colors
# This also includes "official" LEGO colors: http://www.peeron.com/cgi-bin/invcgis/colorguide.cgi
# See also https://www.bartneck.de/2016/09/09/the-curious-case-of-lego-colors/
def linear(srgb: float) -> float:
    if srgb <= 0.04045:
        return srgb / 12.92
    else:
        return ((srgb + 0.055) / 1.055) ** 2.4


rgb_peeron_by_code = {
    # Trans_Black
    40: [linear(191/255), linear(183/255), linear(177/255)],
    # Light_Bluish_Gray
    71: [linear(163/255), linear(162/255), linear(164/255)],
}

# Manually adjusted for this addon.
rgb_ldr_tools_by_code = {
    80: [0.55, 0.55, 0.55],  # Metallic_Silver
    256: [0.015, 0.015, 0.015],  # Rubber_Black
}
