pub enum TextComponent {
    Static(&'static str),
    Owned(String),
}

impl TextComponent {
    pub fn to_json(&self) -> &str {
        match self {
            TextComponent::Static(string) => *string,
            TextComponent::Owned(string) => string,
        }
    }
}

impl From<String> for TextComponent {
    fn from(string: String) -> Self {
        string.as_str().into()
    }
}

impl From<&str> for TextComponent {
    fn from(string: &str) -> Self {
        let mut result = String::new();
        result.push_str("{\"text\": \"");
        let mut last_end = 0;
        // also escape backslashes
        for (start, part) in string.match_indices('"') {
            result.push_str(unsafe { string.get_unchecked(last_end..start) });
            result.push_str("\\\"");
            last_end = start + part.len();
        }
        result.push_str(unsafe { string.get_unchecked(last_end..string.len()) });
        result.push_str("\"}");

        TextComponent::Owned(result)
    }
}

/*pub use text_component_macros::component;

#[test]
pub fn something() {
    component!(
        red! bold! link!("https://twitch.tv/moulberry2") {
            // "Text " yellow!{"here"} " After" arg
        }
    )
}*/

/*

TODO:
let unfinished_component = component! {
    red! bold! link!("https://twitch.tv/moulberry2") {
        "Text " yellow!{"here"} " After" #arg
    }
};

let component = Component::new()
    .color("Red")
    .style(Style::Bold)
    .link("https://twitch.tv/moulberry2")
    .append(
        Component::new()
            .with_text("Text")
            .append(Component::new().with_text("here").color("yellow")
            .append(Component::new().with_text("After")))
            .append(Component::new().with_text(arg)))
    );

*/
