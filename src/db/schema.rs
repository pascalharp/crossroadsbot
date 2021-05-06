table! {
    use diesel::sql_types::*;
    use crate::db::models::*;

    roles (id) {
        id -> Int4,
        title -> Text,
        repr -> Text,
        emoji -> Int8,
        active -> Bool,
    }
}

table! {
    use diesel::sql_types::*;
    use crate::db::models::*;

    signup_roles (id) {
        id -> Int4,
        signup_id -> Int4,
        role_id -> Int4,
    }
}

table! {
    use diesel::sql_types::*;
    use crate::db::models::*;

    signups (id) {
        id -> Int4,
        user_id -> Int4,
        training_id -> Int4,
    }
}

table! {
    use diesel::sql_types::*;
    use crate::db::models::*;

    training_roles (id) {
        id -> Int4,
        training_id -> Int4,
        role_id -> Int4,
    }
}

table! {
    use diesel::sql_types::*;
    use crate::db::models::*;

    trainings (id) {
        id -> Int4,
        title -> Text,
        date -> Timestamp,
        state -> Training_state,
    }
}

table! {
    use diesel::sql_types::*;
    use crate::db::models::*;

    users (id) {
        id -> Int4,
        discord_id -> Int8,
        gw2_id -> Text,
    }
}

joinable!(signup_roles -> roles (role_id));
joinable!(signup_roles -> signups (signup_id));
joinable!(signups -> trainings (training_id));
joinable!(signups -> users (user_id));
joinable!(training_roles -> roles (role_id));
joinable!(training_roles -> trainings (training_id));

allow_tables_to_appear_in_same_query!(
    roles,
    signup_roles,
    signups,
    training_roles,
    trainings,
    users,
);
