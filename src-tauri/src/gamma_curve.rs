use crate::model::{ChannelMode, ChannelSettings, GammaRamp, Profile, RAMP_SIZE};

pub fn generate_ramp(profile: &Profile) -> GammaRamp {
    let linked = profile.linked.clamped();
    let red = channel_values(settings_for(profile.channel_mode, linked, profile.red));
    let green = channel_values(settings_for(profile.channel_mode, linked, profile.green));
    let blue = channel_values(settings_for(profile.channel_mode, linked, profile.blue));

    GammaRamp { red, green, blue }
}

fn settings_for(mode: ChannelMode, linked: ChannelSettings, channel: ChannelSettings) -> ChannelSettings {
    match mode {
        ChannelMode::Linked => linked,
        ChannelMode::Rgb => channel.clamped(),
    }
}

fn channel_values(settings: ChannelSettings) -> Vec<u16> {
    (0..RAMP_SIZE)
        .map(|index| {
            let x = index as f64 / 255.0;
            let gamma_adjusted = x.powf(1.0 / settings.gamma);
            let contrasted = (gamma_adjusted - 0.5) * settings.contrast + 0.5;
            let adjusted = (contrasted + settings.brightness).clamp(0.02, 0.98);
            (adjusted * u16::MAX as f64).round() as u16
        })
        .collect()
}
