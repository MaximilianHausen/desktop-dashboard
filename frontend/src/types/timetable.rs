#[derive(PartialEq, Clone, Debug)]
pub struct Timetable {
    pub times: Vec<Vec<String>>,
    pub columns: Vec<TimetableColumn>,
}

#[derive(PartialEq, Clone, Debug)]
pub struct TimetableColumn {
    pub name: String,
    pub lessons: Vec<Option<Lesson>>,
}

#[derive(PartialEq, Clone, Debug)]
pub struct Lesson {
    pub subject: Subject,
    pub status: LessonStatus,
}

#[derive(PartialEq, Clone, Debug)]
pub struct Subject {
    pub full_name: String,
    pub short_name: String,
    pub teacher: String,
    pub room: String,
    pub color: (u8, u8, u8),
}

#[derive(PartialEq, Clone, Debug)]
pub enum LessonStatus {
    Normal,
    Cancelled,
    Replaced(Subject),
}
