use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let api_key = "API_KEY";

    let client = reqwest::blocking::Client::new();

    let body = serde_json::json!({
        "model": "ark-model-name",
        "messages": [
            {
                "role": "user",
                "content": "Hello, Ark API!"
            }
        ]
    });

    let res = client
        .post("https://ark-api-endpoint/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()?;

    let text = res.text()?;

    println!("Response:\n{}", text);

    Ok(())
}