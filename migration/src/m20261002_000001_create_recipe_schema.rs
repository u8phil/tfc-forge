//! Migration 1: create the normalized recipe schema.
//!
//! Tables:
//!   - `recipes`         one row per saved recipe
//!   - `recipe_rules`    ordered child rows (1..=3 per recipe)
//!   - `recipe_actions`  ordered child rows (0..=256 per recipe)
//!
//! Category/name uniqueness is case-insensitive. SQLite does not offer a
//! portable `COLLATE NOCASE` on a `UNIQUE` table constraint through SeaQuery's
//! table builder, so we persist canonical lower-case key columns and put the
//! uniqueness constraint on `(category_key, name_key)`. `category` and `name`
//! keep the user-facing casing.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_query::{ForeignKeyAction, Index};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Recipes::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Recipes::Id).text().not_null().primary_key())
                    .col(ColumnDef::new(Recipes::Category).text().not_null())
                    .col(ColumnDef::new(Recipes::CategoryKey).text().not_null())
                    .col(ColumnDef::new(Recipes::Name).text().not_null())
                    .col(ColumnDef::new(Recipes::NameKey).text().not_null())
                    .col(ColumnDef::new(Recipes::Start).integer().not_null())
                    .col(ColumnDef::new(Recipes::Target).integer().not_null())
                    .col(ColumnDef::new(Recipes::Compact).text().not_null())
                    .col(ColumnDef::new(Recipes::CreatedAt).text().not_null())
                    .col(ColumnDef::new(Recipes::UpdatedAt).text().not_null())
                    .index(
                        Index::create()
                            .name("ux_recipes_category_name")
                            .col(Recipes::CategoryKey)
                            .col(Recipes::NameKey)
                            .unique(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(RecipeRules::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RecipeRules::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(RecipeRules::RecipeId).text().not_null())
                    .col(ColumnDef::new(RecipeRules::Position).integer().not_null())
                    .col(ColumnDef::new(RecipeRules::RuleType).text().not_null())
                    .col(ColumnDef::new(RecipeRules::RuleOrder).text().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_recipe_rules_recipe")
                            .from(RecipeRules::Table, RecipeRules::RecipeId)
                            .to(Recipes::Table, Recipes::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("ux_recipe_rules_recipe_position")
                    .table(RecipeRules::Table)
                    .col(RecipeRules::RecipeId)
                    .col(RecipeRules::Position)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(RecipeActions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RecipeActions::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(RecipeActions::RecipeId).text().not_null())
                    .col(ColumnDef::new(RecipeActions::Position).integer().not_null())
                    .col(ColumnDef::new(RecipeActions::ActionId).text().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_recipe_actions_recipe")
                            .from(RecipeActions::Table, RecipeActions::RecipeId)
                            .to(Recipes::Table, Recipes::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("ux_recipe_actions_recipe_position")
                    .table(RecipeActions::Table)
                    .col(RecipeActions::RecipeId)
                    .col(RecipeActions::Position)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Child tables first so foreign key references are gone before recipes.
        manager
            .drop_table(Table::drop().table(RecipeActions::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(RecipeRules::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Recipes::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Recipes {
    Table,
    Id,
    Category,
    CategoryKey,
    Name,
    NameKey,
    Start,
    Target,
    Compact,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum RecipeRules {
    Table,
    Id,
    RecipeId,
    Position,
    RuleType,
    RuleOrder,
}

#[derive(DeriveIden)]
enum RecipeActions {
    Table,
    Id,
    RecipeId,
    Position,
    ActionId,
}
