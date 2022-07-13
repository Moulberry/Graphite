use protocol::{types::{ChatVisibility, ArmPosition}, play::client::ClientInformation};

// todo: use the ClientInformation packet instead of this struct
// will require parsing the language into a String in order to
// prevent allocation on client settings load

#[derive(Default)]
pub struct PlayerSettings {
    pub language: String,
    pub view_distance: u8,
    pub chat_visibility: ChatVisibility,
    pub chat_colors: bool,
    pub model_customization: i8,
    pub arm_position: ArmPosition,
    pub text_filtering_enabled: bool,
    pub show_on_server_list: bool
}

impl PlayerSettings {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn update(&mut self, packet: ClientInformation) {
        self.language.clear();
        self.language.push_str(packet.language);

        self.view_distance = packet.view_distance;
        self.chat_visibility = packet.chat_visibility;
        self.chat_colors = packet.chat_colors;
        self.model_customization = packet.model_customization;
        self.arm_position = packet.arm_position;
        self.text_filtering_enabled = packet.text_filtering_enabled;
        self.show_on_server_list = packet.show_on_server_list;
    }
}