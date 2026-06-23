use crate::model::{ChannelMode, ChannelSettings, GammaRamp, Profile, RAMP_SIZE};

pub fn generate_ramp(profile: &Profile, baseline: &GammaRamp) -> GammaRamp {
    let linked = profile.linked.clamped();
    let fallback = GammaRamp::linear();
    let baseline = if baseline.is_valid() {
        baseline
    } else {
        &fallback
    };
    let red = channel_values(
        settings_for(profile.channel_mode, linked, profile.red),
        &baseline.red,
    );
    let green = channel_values(
        settings_for(profile.channel_mode, linked, profile.green),
        &baseline.green,
    );
    let blue = channel_values(
        settings_for(profile.channel_mode, linked, profile.blue),
        &baseline.blue,
    );

    GammaRamp { red, green, blue }
}

fn settings_for(
    mode: ChannelMode,
    linked: ChannelSettings,
    channel: ChannelSettings,
) -> ChannelSettings {
    match mode {
        ChannelMode::Linked => linked,
        ChannelMode::Rgb => channel.clamped(),
    }
}

fn channel_values(settings: ChannelSettings, baseline: &[u16]) -> Vec<u16> {
    baseline
        .iter()
        .take(RAMP_SIZE)
        .map(|value| {
            let x = *value as f64 / u16::MAX as f64;
            let gamma_adjusted = x.powf(1.0 / settings.gamma);
            let contrasted = (gamma_adjusted - 0.5) * settings.contrast + 0.5;
            let adjusted = (contrasted + settings.brightness).clamp(0.0, 1.0);
            (adjusted * u16::MAX as f64).round() as u16
        })
        .collect()
}
