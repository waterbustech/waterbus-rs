use diesel::PgConnection;

pub struct Repository<'a> {
    pub conn: &'a mut PgConnection,
}
