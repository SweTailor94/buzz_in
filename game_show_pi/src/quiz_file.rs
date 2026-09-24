use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Serialize)]
pub struct Quiz {
    #[serde(rename = "Name")]
    pub name: String,
    pub questions: Vec<Question>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Question {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shall_zoom: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question_sound: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correct_sound: Option<String>,
    pub text: String,
    pub answer: String,
}

pub fn get_quiz(filename: &PathBuf) -> anyhow::Result<Quiz> {
    println!("Reading quiz from {}", filename.display());
    let mut quiz: Quiz = serde_json::from_str::<Quiz>(&std::fs::read_to_string(filename)?)?;
    let path = filename.parent().unwrap();
    println!("Opened: {}", quiz.name);
    println!("Number of quiz questions: {}", quiz.questions.len());
    for (i, question) in quiz.questions.iter_mut().enumerate() {
        if let Some(file) = &question.image {
            question.image = Some(path.join(Path::new(&file)).display().to_string());
        }
        if let Some(file) = &question.question_sound {
            question.question_sound = Some(path.join(Path::new(&file)).display().to_string());
        }
        if let Some(file) = &question.correct_sound {
            question.correct_sound = Some(path.join(Path::new(&file)).display().to_string());
        }
        println!(
            "Question {}: {}",
            i + 1,
            if let Some(image) = &question.image {
                image.clone()
            } else {
                "does not have image".to_string()
            }
        );
    }
    println!();
    Ok(quiz)
}
