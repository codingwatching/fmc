use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use std::collections::HashMap;

use super::{Item, ItemId, ItemStack, Items};

mod shaped;

pub struct CraftingPlugin;
impl Plugin for CraftingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_recipes);
    }
}

fn load_recipes(mut commands: Commands, items: Res<Items>) {
    let mut recipes = HashMap::new();

    let directory = std::fs::read_dir("resources/client/items/recipes").expect(
        "Couldn't read files from recipe directory make sure it is present at 
                resources/client/items/recipes",
    );

    for dir_entry in directory {
        let file_path = match dir_entry {
            Ok(d) => d.path(),
            Err(e) => panic!("Failed to read the filename of a recipe\nError: {}", e),
        };

        let file = match std::fs::File::open(&file_path) {
            Ok(f) => f,
            Err(e) => panic!(
                "Failed to open recipe at path: {}\nError: {}",
                &file_path.display(),
                e
            ),
        };

        let item_recipes: Vec<RecipeJson> = match serde_json::from_reader(file) {
            Ok(i) => i,
            Err(e) => panic!(
                "Failed to read item recipe in file: {}\nError:{}",
                file_path.display(),
                e
            ),
        };

        for recipe_json in item_recipes.into_iter() {
            match recipe_json.pattern_type.as_str() {
                "shaped" => {
                    let (pattern, required_amount): (Vec<Vec<Option<ItemId>>>, Vec<Vec<u32>>) =
                        match &recipe_json.pattern {
                            PatternJson::Grid(pattern) => pattern
                                .iter()
                                .map(|row| {
                                    row.iter()
                                        .map(|(name, amount)| match name.as_str() {
                                            // Empty part of pattern
                                            "" => (None, 0),
                                            // Item part of pattern
                                            _ => match items.ids.get(name) {
                                                Some(id) => (Some(*id), *amount),
                                                None => panic!(
                                                    "Error parsing item recipe pattern at: {}\n\
                                                        Item name '{}' is not recognized",
                                                    file_path.display(),
                                                    name
                                                ),
                                            },
                                        })
                                        .unzip()
                                })
                                .unzip(),
                            _ => panic!(
                            "Error parsing item recipe pattern at: {}\n'pattern_type' is 'shaped',\
                                    but the pattern is not in the form of a grid. Should be like:\n\
                                    [\n    [[\"\", 0], [\"item\", 1]],\n    \
                                           [[\"item\", 1], [\"\", 0]]\n\
                                    ]\n",
                            file_path.display()
                        ),
                        };

                    let output_item = match items.ids.get(&recipe_json.output_item) {
                        Some(id) => *id,
                        None => panic!(
                            "Error parsing item recipe pattern at: {}\n Item name '{}'\
                            is not recognized",
                            file_path.display(),
                            &recipe_json.output_item
                        ),
                    };

                    let recipe = shaped::Recipe {
                        required_amount,
                        output_item: Item {
                            id: output_item,
                            properties: serde_json::Value::Object(serde_json::Map::new()),
                        },
                        output_amount: recipe_json.output_amount,
                        data: recipe_json.data,
                    };

                    recipes
                        .entry(recipe_json.collection_name)
                        .or_insert(RecipeCollection::default())
                        .insert(
                            Pattern::Shaped(shaped::Pattern { inner: pattern }),
                            Recipe::Shaped(recipe),
                        );
                }
                _ => (),
            }
        }
    }

    commands.insert_resource(Recipes {
        collections: recipes,
    })
}

#[derive(Serialize, Deserialize)]
struct RecipeJson {
    collection_name: String,
    pattern_type: String,
    pattern: PatternJson,
    output_item: String,
    output_amount: u32,
    #[serde(default)]
    data: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum PatternJson {
    Grid(Vec<Vec<(String, u32)>>),
    List(Vec<(String, u32)>),
}

#[derive(Serialize, Deserialize)]
enum RequiredJson {
    Grid(Vec<Vec<u32>>),
    List(Vec<u32>),
}

// TODO: Unshaped and Unordered are the same right? merge if
// All recipe types share the same interface, but differ in the way they handle the input.
pub enum Recipe {
    /// Square crafting area where the item position matters.
    Shaped(shaped::Recipe),
    //// Square crafting area where the item position doesn't matter.
    //Unshaped(UnshapedRecipe),
    //// List of crafting items where the order matters
    //Ordered(OrderedRecipe)
    //// List of crafting items where the order doesn't matter
    //Unordered(UnorderedRecipe)
}

impl Recipe {
    // TODO: This should return Option<ItemStack>, need to make the Items a global I think.
    //
    /// Craft items by consuming the input. Will produce 'amount' times the recipe output amount
    /// items (or as many as possible if amount is more than is possible).
    /// DOES NOT TEST THAT THE INPUT MATCHES
    pub fn craft(&self, input: &mut [ItemStack], amount: u32) -> Option<(Item, u32)> {
        return match self {
            Recipe::Shaped(r) => r.craft(input, amount),
        };
    }

    /// Get how many of the output item can be crafted given the input.
    fn get_craftable_amount(&self, input: &[ItemStack]) -> u32 {
        return match self {
            Recipe::Shaped(r) => r.get_craftable_amount(input),
        };
    }

    /// The item that can be crafted through the recipe.
    pub fn output_item(&self) -> &Item {
        match self {
            Recipe::Shaped(s) => s.output_item(),
        }
    }

    /// The amount of items that can be created from the recipe
    fn output_amount(&self) -> u32 {
        match self {
            Recipe::Shaped(s) => s.output_amount,
        }
    }

    // TODO: Idk what this was for
    pub fn data(&self) -> &serde_json::Value {
        return match self {
            Recipe::Shaped(s) => s.data(),
        };
    }
}

#[derive(Hash, PartialEq, Eq)]
enum Pattern {
    Shaped(shaped::Pattern),
}

// TODO: Not all recipe collections contain all the Pattern enumerations. Because of this the enumerations are behind a flag.
//       This was dumb maybe, did it to cut down on pattern construction on lookup.
// way dumb
/// A subset of recipes that can be crafted together. i.e all recipes that can be crafted in the
/// crafting table.
#[derive(Default)]
pub struct RecipeCollection {
    shaped: bool,
    recipes: HashMap<Pattern, Recipe>,
}

impl RecipeCollection {
    fn insert(&mut self, pattern: Pattern, recipe: Recipe) {
        match pattern {
            Pattern::Shaped(_) => {
                self.shaped = true;
                self.recipes.insert(pattern, recipe);
            }
        }
    }

    pub fn get_recipe(&self, input: &[ItemStack]) -> Option<&Recipe> {
        if self.shaped {
            let pattern = Pattern::Shaped(shaped::Pattern::from(input));
            return self.recipes.get(&pattern);
        }
        return None;
    }

    /// Get which item and how many can be crafted from the input.
    pub fn get_output(&self, input: &[ItemStack]) -> Option<(&Item, u32)> {
        if let Some(recipe) = self.get_recipe(input) {
            let can_craft = recipe.get_craftable_amount(input);
            if can_craft > 0 {
                return Some((recipe.output_item(), recipe.output_amount()));
            }
        }

        return None;
    }
}

/// Holds all recipes in the game.
#[derive(Resource)]
pub struct Recipes {
    collections: HashMap<String, RecipeCollection>,
}

impl Recipes {
    pub fn get(&self, collection_name: &str) -> &RecipeCollection {
        return match self.collections.get(collection_name) {
            Some(c) => c,
            None => panic!(
                "No recipes found for the collection name: {}",
                collection_name
            ),
        };
    }
}
