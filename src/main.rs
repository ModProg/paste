use bonsaidb::core::schema;

#[derive(schema::Schema, Debug)]
#[schema(name = "")]
struct Schema;

#[tokio::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));
}
