pub const HTML_DIR_LIST_START: &str = r#"
<!DOCTYPE html>
<html lang="">

<head>
  <meta charset="utf-8">
  <title>webdev directory listing</title>

  <link rel="icon" href="data:,">

  <meta name="viewport" content="width=device-width, initial-scale=1">

  <style>
  body { 
    display: flex;
    flex-direction: column;
    font-size: 16px;
    gap: .5rem;
  }
  a { 
    display: block;
    padding: .25rem 1rem;
    font-size: 1.125rem;
  }
  </style>
</head>

<body>
"#;

pub const HTML_DIR_LIST_END: &str = r#"
</body>

</html>
"#;
