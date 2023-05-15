use handlebars::{Context, Handlebars, Helper, Output, RenderContext, RenderError};
use passwords::PasswordGenerator;

pub(crate) fn rand_pass_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    rc: &mut RenderContext,
    _: &mut dyn Output,
) -> Result<(), RenderError> {
    // get parameter from helper or throw an error
    let key = h.param(0).ok_or(RenderError::new(
        "Param 0 is required for rand_pass helper.",
    ))?;
    let key = key
        .value()
        .as_str()
        .ok_or(RenderError::new("Param 0 must be string."))?;
    if key.is_empty() {
        return Err(RenderError::new("Param 0 can't be empty."));
    }
    // get parameter from helper or throw an error
    let pass_len = h.param(1).ok_or(RenderError::new(
        "Param 0 is required for rand_pass helper.",
    ))?;
    let pass_len = pass_len
        .value()
        .as_u64()
        .ok_or(RenderError::new("Param 1 must be integer."))?;
    let pg = PasswordGenerator {
        length: pass_len as usize,
        numbers: true,
        lowercase_letters: true,
        uppercase_letters: true,
        symbols: true,
        spaces: false,
        exclude_similar_characters: false,
        strict: true,
    };
    let password = pg
        .generate_one()
        .map_err(|err| RenderError::new(format!("Can't generate password: {err:#}.")))?;
    let block = rc.block_mut().unwrap();
    block.set_local_var(key, password.into());
    Ok(())
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use handlebars::no_escape;

    use super::*;

    #[test]
    fn test_rand_pass_len() {
        let mut handlebars = Handlebars::new();
        handlebars.register_escape_fn(no_escape);
        handlebars.set_strict_mode(true);
        handlebars.register_helper("rand_pass", Box::new(rand_pass_helper));
        handlebars
            .register_template_string("test", r#"{{rand_pass "my_pass" 20}}{{@my_pass}}"#)
            .unwrap();

        let data: HashMap<String, String> = HashMap::new();
        assert_eq!(20, handlebars.render("test", &data).unwrap().len());
    }
}
