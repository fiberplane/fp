use anyhow::Result;
use cli_table::format::*;
use cli_table::{print_stdout, Row, Table, Title};
use std::io::{LineWriter, Write};

pub fn output_list<T, R>(input: T) -> Result<()>
where
    T: IntoIterator<Item = R>,
    R: Row + Title,
{
    print_stdout(
        input
            .table()
            .title(R::title())
            .border(Border::builder().build())
            .separator(Separator::builder().build()),
    )
    .map_err(Into::into)
}

pub fn output_details<T, R>(args: T) -> Result<()>
where
    T: IntoIterator<Item = R>,
    R: Row,
{
    print_stdout(
        args.table()
            .border(Border::builder().build())
            .separator(Separator::builder().build()),
    )
    .map_err(Into::into)
}

#[derive(Table)]
pub struct GenericKeyValue {
    #[table(title = "key", justify = "Justify::Right")]
    key: String,

    #[table(title = "value")]
    value: String,
}

impl GenericKeyValue {
    #[must_use]
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

pub fn output_string_list<T>(input: T) -> Result<()>
where
    T: IntoIterator<Item = String>,
{
    let mut writer = LineWriter::new(std::io::stdout());
    for line in input.into_iter() {
        writer.write_all(line.as_bytes())?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}
