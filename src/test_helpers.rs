use crate::auth::NadeoCredentials;

pub fn get_test_creds() -> NadeoCredentials {
    dotenv::dotenv().expect("Failed to read .env file");
    // let u = env!("NADEO_API_TEST_DS_USERNAME").to_string();
    // let p = env!("NADEO_API_TEST_DS_PASSWORD").to_string();
    let u =
        std::env::var("NADEO_API_TEST_DS_USERNAME").expect("NADEO_API_TEST_DS_USERNAME not set");
    let p =
        std::env::var("NADEO_API_TEST_DS_PASSWORD").expect("NADEO_API_TEST_DS_PASSWORD not set");
    NadeoCredentials::DedicatedServer { u, p }
}

pub fn get_test_ubi_creds() -> NadeoCredentials {
    dotenv::dotenv().expect("Failed to read .env file");
    let u = std::env::var("NADEO_API_TEST_UBI_EMAIL").expect("NADEO_API_TEST_UBI_EMAIL not set");
    let p =
        std::env::var("NADEO_API_TEST_UBI_PASSWORD").expect("NADEO_API_TEST_UBI_PASSWORD not set");
    NadeoCredentials::ubisoft(&u, &p)
}

pub fn get_test_email() -> String {
    dotenv::dotenv().expect("Failed to read .env file");
    std::env::var("NADEO_TEST_UA_EMAIL").expect("NADEO_TEST_UA_EMAIL not set")
}
