use nanoid::nanoid;
use rand::Rng;

pub fn generate_meeting_code() -> i32 {
    let mut rng = rand::rng();
    let mut id = String::new();

    while id.len() != 9 || id.starts_with('0') {
        id.clear();
        for _ in 0..9 {
            let digit = rng.random_range(1..=9).to_string();
            id.push_str(&digit);
        }
    }

    let code = id.parse().unwrap();

    code
}

pub fn generate_username() -> String {
    nanoid!(12)
}
