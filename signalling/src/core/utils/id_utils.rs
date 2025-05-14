use nanoid::nanoid;
use rand::{Rng, distr::Alphanumeric, rng};

pub fn generate_room_code() -> String {
    let mut rng = rng();

    fn random_alpha_string(len: usize, rng: &mut impl Rng) -> String {
        let mut result = String::new();
        while result.len() < len {
            let c = rng.sample(Alphanumeric);
            let c = (c as char).to_ascii_lowercase();
            if c.is_ascii_alphabetic() {
                result.push(c);
            }
        }
        result
    }

    format!(
        "{}-{}-{}",
        random_alpha_string(3, &mut rng),
        random_alpha_string(4, &mut rng),
        random_alpha_string(3, &mut rng),
    )
}

pub fn generate_username() -> String {
    nanoid!(12)
}
