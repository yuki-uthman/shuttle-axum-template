use api_lib::build_router;
use config::{Config as ConfigCrate, File};
use sqlx::{Connection, Executor, PgConnection, PgPool};

const CONFIG_FILE: &str = "dev.yaml";

#[derive(serde::Deserialize, Debug)]
struct Application {
    port: u16,
    host: String,
}

impl Application {
    fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(serde::Deserialize, Debug)]
struct Database {
    username: String,
    password: String,
    host: String,
    port: u16,
    database_name: String,
}

impl Database {
    pub fn connection_string(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.username, self.password, self.host, self.port, self.database_name
        )
    }

    /// Omitting the database name connects to the Postgres instance, not a specific logical database.
    /// This is useful for operations that create or drop databases.
    pub fn connection_string_without_db(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}",
            self.username, self.password, self.host, self.port
        )
    }
}

#[derive(serde::Deserialize, Debug)]
struct Config {
    application: Application,
    database: Database,
}

fn get_config() -> Config {
    let base_dir = std::env::current_dir().unwrap();
    let config_path = base_dir.join(CONFIG_FILE);
    let config = ConfigCrate::builder()
        .add_source(File::with_name(config_path.to_str().unwrap()))
        .build()
        .unwrap();

    config.try_deserialize::<Config>().unwrap()
}

async fn start_database(config: &mut Config) -> PgPool {
    config.database.database_name = uuid::Uuid::new_v4().to_string();

    // Create database
    let mut connection = PgConnection::connect(&config.database.connection_string_without_db())
        .await
        .expect("Failed to connect to Postgres");
    connection
        .execute(&*format!(
            r#"CREATE DATABASE "{}";"#,
            config.database.database_name
        ))
        .await
        .expect("Failed to create database.");

    let connection_string = config.database.connection_string();
    let pool = PgPool::connect(&connection_string)
        .await
        .expect("Failed to connect to Postgres");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    pool
}

pub struct App {
    config: Config,
}

impl App {
    pub fn address(&self) -> String {
        self.config.application.address()
    }
}

pub async fn spawn_app() -> App {
    let mut config = get_config();
    config.application.port = 0;

    let pool = start_database(&mut config).await;

    let app = build_router(pool);

    let listener = tokio::net::TcpListener::bind(config.application.address())
        .await
        .unwrap();
    config.application.port = listener.local_addr().unwrap().port();

    tokio::spawn(async {
        axum::serve(listener, app).await.unwrap();
    });

    App { config }
}
