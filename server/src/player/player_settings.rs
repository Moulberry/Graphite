use protocol::{
    play::client::ClientInformation,
    types::{ArmPosition, ChatVisibility},
};

// todo: use the ClientInformation packet instead of this struct
// will require parsing the language into a String in order to
// prevent allocation on client settings load

pub struct PlayerSettings {
    pub language: String,
    pub brand: String,
    pub view_distance: u8,
    pub chat_visibility: ChatVisibility,
    pub chat_colors: bool,
    pub model_customization: i8, // todo: bitset
    pub arm_position: ArmPosition,
    pub text_filtering_enabled: bool,
    pub show_on_server_list: bool,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        Self {
            language: String::from("en_us"),
            brand: String::from("null"),
            view_distance: 8,
            chat_visibility: Default::default(),
            chat_colors: true,
            model_customization: Default::default(),
            arm_position: Default::default(),
            text_filtering_enabled: true,
            show_on_server_list: true,
        }
    }
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

    pub fn set_brand(&mut self, brand: &str) {
        self.brand.clear();
        self.brand.push_str(brand);
    }
}
