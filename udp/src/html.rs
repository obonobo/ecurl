//! This module contains the webpage stuff for the dir listing of the file
//! server

use std::net::TcpStream;

use crate::transport::{JoinIter, UdpxStream};

/// Polymorphic [template] function.
///
/// This is a tool for swapping the template implementation depending on if we
/// are using TCP or UDPx.
pub trait Templater {
    fn template(&self, files: impl IntoIterator<Item = String>) -> String;
}

impl Templater for TcpStream {
    fn template(&self, files: impl IntoIterator<Item = String>) -> String {
        let links = files
            .into_iter()
            .map(|file| format!("    <a href=\"{}\">{}</a>\n", file, file))
            .collect::<String>();
        HTML.replacen("    {LINKS}", links.as_str(), 1)
    }
}

impl Templater for UdpxStream {
    fn template(&self, files: impl IntoIterator<Item = String>) -> String {
        files.into_iter().join("\n")
    }
}

/// This is the html document that is returned by the dir listing function
pub const HTML: &str = r#"
<!DOCTYPE html>
<html>

<head>
  <meta charset="utf-8">
  <meta http-equiv="X-UA-Compatible" content="IE=edge">
  <title>HTTPFS</title>
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <style>
    @import url("https://fonts.googleapis.com/css2?family=Red+Hat+Mono:ital,wght@1,500&display=swap");
    @import url("https://fonts.googleapis.com/css2?family=Ubuntu:wght@300&display=swap");

    body {
      padding: 1rem;
      font-family: "Ubuntu", sans-serif;
    }

    h1>a {
      user-select: none;
      font-family: "Red Hat Mono", monospace;
      color: rgb(93, 134, 148);
      font-size: 3rem;
      text-decoration: underline;
      text-decoration-thickness: 4px;
    }

    p {
      display: flex;
      flex-direction: column;
    }

    p>* {
      margin: 0.3em 0;
      /*
      transition: all cubic-bezier(0.075, 0.82, 0.165, 1);
      */
    }

    p>a:hover {
     /*
     transform: scale(1.1);
     */
    }

    #drop-zone {
      user-select: none;
      padding: 0.5em;
      display: flex;
      place-content: center;
      place-items: center;
      border: 4px dashed lightblue;
      border-radius: 10px;
      width: 200px;
      height: 125px;
      color: rgb(124, 165, 179);
      font-size: 1.5em;
      font-weight: bold;
    }
  </style>
</head>

<body>
  <h1><a href="/">HTTPFS</a></h1>
  <p>
    <a href=".">./</a>
    <a href="..">../</a>
    {LINKS}
  </p>
  <div id="drop-zone" ondrop="dropHandler(event);" ondragover="dragOverHandler(event);">
    <p>Drag and Drop</p>
  </div>
  <script>
    function dragOverHandler(ev) {
      ev.preventDefault();
    }

    function dropHandler(ev) {
      ev.preventDefault();

      const processFile = (file) => {
        console.log(`Uploading file: ${file.name}`);
        fetch(window.location.pathname + file.name, {
          method: "POST",
          body: file,
        })
          .then((res) => {
            console.log(`Success! ${res}`);
            location.reload();
          })
          .catch(console.log);
      };

      if (ev.dataTransfer.items) {
        for (var i = 0; i < ev.dataTransfer.items.length; i++) {
          if (ev.dataTransfer.items[i].kind === "file") {
            var file = ev.dataTransfer.items[i].getAsFile();
            processFile(file);
          }
        }
      } else {
        for (var i = 0; i < ev.dataTransfer.files.length; i++) {
          let file = ev.dataTransfer.files[i];
          processFile(file);
        }
      }
    }
  </script>
</body>

</html>
"#;
