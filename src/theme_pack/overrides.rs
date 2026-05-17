use crate::config::ThemeConfig;

pub(crate) fn apply_user_theme_overrides(theme: &mut ThemeConfig, overrides: &ThemeConfig) {
    let defaults = ThemeConfig::default();
    macro_rules! override_string {
        ($field:ident) => {
            if overrides.$field != defaults.$field {
                theme.$field = overrides.$field.clone();
            }
        };
    }
    macro_rules! override_copy {
        ($field:ident) => {
            if overrides.$field != defaults.$field {
                theme.$field = overrides.$field;
            }
        };
    }

    override_string!(shelf_top);
    override_string!(shelf_bottom);
    override_string!(shelf_stroke);
    override_string!(shelf_highlight);
    override_string!(indicator);
    override_string!(badge);
    override_copy!(reflection_opacity);
    override_copy!(reflection_height);
    override_copy!(shelf_height_ratio);
    override_copy!(shelf_slant_ratio);
    override_copy!(icon_gap_ratio);
    override_copy!(side_margin_ratio);
    override_copy!(shelf_horizon_ratio);
    override_copy!(front_lip_ratio);
    override_copy!(reflection_band_ratio);
    override_copy!(tilt);
    override_copy!(depth);
    override_copy!(bevel);
    override_copy!(floor_opacity);
    override_copy!(shadow_strength);
    override_copy!(highlight_strength);
    override_copy!(reflection_blur);
    override_copy!(material_roughness);
    override_copy!(icon_floor_offset);
}
