use serde_json::Value;

pub mod connection;
pub mod issuance;

pub mod proof;

#[derive(Clone, Debug, Default)]
pub struct Messages {
    pub messages: Vec<String>,
}
#[derive(Clone, Debug, Default)]
pub struct TypeMessage {
    pub ty: String,
    pub content: String,
}
pub fn receive_msgs(id: String) -> Messages {
    let Ok(response) = ureq::get(&format!("https://did-relay.ubique.ch/get_msg/{id}"))
        .call() else {
            return Messages::default()
        };
    let Ok(values) = response.into_json::<Vec<Value>>() else {
        return Messages::default()
    };
    let messages: Vec<String> = values
        .into_iter()
        .map(|v| serde_json::to_string(&v).unwrap())
        .collect::<Vec<_>>();
    Messages { messages }
}
