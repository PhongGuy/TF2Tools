//! A synthetic HUD option schema over the switch fixture copy.

pub const SCHEMA: &str = r##"{
  // a comment line
  "Links": { "GitHub": "https://github.com/Someone/SomeHUD" },
  "CustomizationsFolder": "#customization",
  "EnabledFolder": "#customization//_enabled",
  "Controls": {
    "Colours": [
      { "Name": "sh_tan", "Label": "Tan", "Type": "ColorPicker", "Value": "1 2 3 255", "Restart": true, "Shadow": true, "Pulse": true,
        "Files": { "resource/clientscheme.res": { "Scheme": { "Colors": { "TanLight": "$value" } } } } }
    ],
    "Chat": [
      { "Name": "sh_chat_top", "Label": "Chat at top", "Type": "CheckBox", "Value": "false",
        "Files": { "resource/ui/basechat.res": { "HudChat": { "ypos": { "true": "0", "false": "275" } } } } },
      { "Name": "sh_rows", "Label": "Rows", "Type": "IntegerUpDown", "Value": "12",
        "Files": { "scripts/hudlayout.res": { "HudDeathNotice": { "MaxDeathNotices": "$value" } } } }
    ],
    "Health": [
      { "Name": "sh_big", "Label": "Big health", "Type": "Checkbox", "Value": "false", "FileName": "bh_player_largehealth.res" },
      { "Name": "sh_style", "Label": "Style", "Type": "ComboBox", "Value": "0", "Options": [
          { "Label": "Plain", "Value": "0" },
          { "Label": "Streamer", "Value": "1", "RenameFile": { "OldName": "customizations/streamermode_/", "NewName": "customizations/streamermode/" } } ] }
    ],
    "Left out": [
      { "Name": "sh_cfg", "Label": "Cfg", "Type": "CheckBox", "WriteCfg": { "FileName": "x.cfg" } },
      { "Name": "sh_bg", "Label": "Background", "Type": "CustomBackground", "Special": "CustomBackground" },
      { "Name": "sh_climb", "Label": "Climb", "Type": "CheckBox", "Files": { "../../cfg/x.res": { "A": { "b": "1" } } } },
      { "Name": "sh_tern", "Label": "Ternary", "Type": "CheckBox", "Files": { "resource/ui/basechat.res": { "HudChat": { "font": "{$sh_chat_top ? A : B}" } } } },
      { "Name": "sh_anim", "Label": "Anim", "Type": "CheckBox", "Files": { "scripts/hudanimations_custom.txt": { "replace": ["a", "b"] } } },
      { "Name": "sh_tan", "Label": "Duplicate", "Type": "CheckBox" }
    ],
    "Misfit": [
      { "Name": "sh_gone", "Label": "Gone", "Type": "CheckBox", "Files": { "resource/ui/gone.res": { "X": { "visible": { "true": "0", "false": "1" } } } } },
      { "Name": "sh_noblock", "Label": "No block", "Type": "CheckBox", "Files": { "resource/ui/basechat.res": { "NoSuchBlock": { "visible": { "true": "0", "false": "1" } } } } }
    ]
  }
}"##;

pub fn schema() -> tf2tools_core::HudSchema {
    tf2tools_core::HudSchema::parse("somehud", SCHEMA.as_bytes()).unwrap()
}
