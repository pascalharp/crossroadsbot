table! {
    roles (id) {
        id -> Int4,
        title -> Text,
        repr -> Text,
        emoji -> Int8,
    }
}

table! {
    signup_roles (id) {
        id -> Int4,
        signup_id -> Int4,
        role_id -> Int4,
    }
}

table! {
    signups (id) {
        id -> Int4,
        user_id -> Int4,
        training_id -> Int4,
    }
}

table! {
    training_roles (id) {
        id -> Int4,
        training_id -> Int4,
        role_id -> Int4,
    }
}

table! {
    trainings (id) {
        id -> Int4,
        title -> Text,
        date -> Timestamp,
        open -> Bool,
    }
}

table! {
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
