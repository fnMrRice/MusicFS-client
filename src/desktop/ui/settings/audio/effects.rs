use crate::{
    i18n::tr,
    ui::style::{self, ComboBoxMotion},
};
use eframe::egui::{self, Ui};
use musicfs_player_core::audio::{
    Effect, EffectKind, LocalDsp, EFFECTS, FREQUENCIES, MAX_EFFECTS, PRESETS,
};

struct DraggedEffect(usize);
pub(super) fn edit(ui: &mut Ui, dsp: &mut LocalDsp, selected: &mut EffectKind) -> bool {
    let mut changed = false;
    let count = dsp.chain.len();
    ui.horizontal(|ui| {
        ui.strong(tr("效果顺序"));
        ui.weak(
            tr("{count} / {max} 个效果")
                .replace("{count}", &count.to_string())
                .replace("{max}", &MAX_EFFECTS.to_string()),
        );
    });
    if count == 0 {
        ui.weak(tr("尚未添加本地效果。添加效果即可实时试听。"));
    }
    let mut reorder = None;
    let mut remove = None;
    for (index, effect) in dsp.chain.iter_mut().enumerate() {
        let (_, payload) =
            ui.dnd_drop_zone::<DraggedEffect, _>(egui::Frame::group(ui.style()), |ui| {
                ui.push_id(("effect", index), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.dnd_drag_source(ui.id().with("handle"), DraggedEffect(index), |ui| {
                            ui.label("⠿")
                        });
                        ui.strong(format!("{}  {}", index + 1, tr(effect.kind.name())));
                        let enabled = if effect.enabled { "开启" } else { "关闭" };
                        changed |= style::checkbox(ui, &mut effect.enabled, tr(enabled)).changed();
                        if style::add_button(ui, index > 0, egui::Button::new("↑"))
                            .on_hover_text(tr("上移"))
                            .clicked()
                        {
                            reorder = Some((index, index - 1));
                        }
                        if style::add_button(ui, index + 1 < count, egui::Button::new("↓"))
                            .on_hover_text(tr("下移"))
                            .clicked()
                        {
                            reorder = Some((index, index + 1));
                        }
                        if style::small_button(ui, tr("删除")).clicked() {
                            remove = Some(index);
                        }
                    });
                    style::collapsing(
                        ui,
                        egui::CollapsingHeader::new(tr("参数")).id_salt("parameters"),
                        |ui| {
                            if effect.kind == EffectKind::GraphicEqualizer {
                                changed |= equalizer(ui, effect);
                            } else {
                                for (param, value) in
                                    effect.kind.parameters().iter().zip(&mut effect.values)
                                {
                                    changed |= ui
                                        .add(
                                            egui::Slider::new(value, param.min..=param.max)
                                                .step_by(param.step as f64)
                                                .suffix(param.unit)
                                                .text(tr(param.name)),
                                        )
                                        .changed();
                                }
                            }
                        },
                    );
                });
            });
        if let Some(from) = payload {
            reorder = Some((from.0, index));
        }
    }
    if let Some(i) = remove {
        dsp.chain.remove(i);
        changed = true;
    } else if let Some((from, to)) =
        reorder.filter(|(from, to)| from != to && *from < count && *to < count)
    {
        let effect = dsp.chain.remove(from);
        dsp.chain.insert(to, effect);
        changed = true;
    }
    ui.horizontal_wrapped(|ui| {
        egui::ComboBox::from_label(tr("选择效果"))
            .selected_text(tr(selected.name()))
            .show_animated(ui, |ui| {
                for kind in EFFECTS {
                    style::selectable_value(ui, selected, kind, tr(kind.name()));
                }
            });
        if style::add_button(ui, count < MAX_EFFECTS, egui::Button::new(tr("添加效果"))).clicked()
        {
            dsp.chain.push(Effect::new(*selected));
            changed = true;
        }
        if style::button(ui, tr("恢复本地默认")).clicked() {
            *dsp = LocalDsp::default();
            changed = true;
        }
    });
    changed
}
fn equalizer(ui: &mut Ui, effect: &mut Effect) -> bool {
    let mut changed = false;
    let active = PRESETS
        .iter()
        .find(|(_, gains)| effect.values[..10] == *gains && effect.values[10..] == [0., 0.])
        .map(|(label, _)| *label)
        .unwrap_or("自定义");
    ui.horizontal_wrapped(|ui| {
        egui::ComboBox::from_label(tr("均衡器预设"))
            .selected_text(tr(active))
            .show_animated(ui, |ui| {
                for (label, gains) in PRESETS {
                    if style::selectable(ui, label == active, tr(label)).clicked() {
                        effect.values[..10].copy_from_slice(&gains);
                        effect.values[10..].fill(0.);
                        changed = true;
                    }
                }
            });
        if style::button(ui, tr("重置均衡器")).clicked() {
            effect.values.fill(0.);
            changed = true;
        }
    });
    egui::ScrollArea::horizontal()
        .id_salt("bands")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, hz) in FREQUENCIES.iter().enumerate() {
                    ui.push_id(i, |ui| {
                        ui.vertical(|ui| {
                            ui.label(format!("{:+.1}", effect.values[i]));
                            let response = ui.add(
                                egui::Slider::new(&mut effect.values[i], -12.0..=12.)
                                    .step_by(0.5)
                                    .vertical()
                                    .show_value(false),
                            );
                            changed |= response.on_hover_text(format!("{hz} Hz")).changed();
                            ui.small(if *hz >= 1000. {
                                format!("{}k", hz / 1000.)
                            } else {
                                hz.to_string()
                            });
                        })
                    });
                }
            });
        });
    for (p, value) in effect.kind.parameters()[10..]
        .iter()
        .zip(&mut effect.values[10..])
    {
        changed |= ui
            .add(
                egui::Slider::new(value, p.min..=p.max)
                    .step_by(0.5)
                    .text(tr(p.name))
                    .suffix(" dB"),
            )
            .changed();
    }
    changed
}
