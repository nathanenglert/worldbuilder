//! The agent surface, driven the way an agent drives it.
//!
//! These go over a real MCP connection rather than calling the handler functions
//! directly, because most of what can break here breaks *between* the model and the
//! code: a schema that will not generate, a tool name that collides, an argument that
//! deserializes to the wrong variant. Calling the Rust function proves none of that.

use std::fs;
use std::path::{Path, PathBuf};

use rmcp::model::CallToolRequestParams;
use rmcp::service::{RoleClient, RunningService};
use rmcp::{ServiceExt, transport::IntoTransport};
use serde_json::{Value, json};
use wb_mcp::WorldServer;

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/vashen")
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), &dest).unwrap();
        }
    }
}

/// A throwaway copy of the example world. Filing a proposal writes a file, and a test
/// that leaves one behind changes the answers of every test after it.
fn scratch_world(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("wb-mcp-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    copy_dir(&example_root(), &dir);
    dir
}

struct Agent {
    client: RunningService<RoleClient, ()>,
}

impl Agent {
    async fn connect(root: PathBuf) -> Self {
        let (server_side, client_side) = tokio::io::duplex(1 << 16);
        let server = WorldServer::open(&root).expect("open world");

        tokio::spawn(async move {
            let Ok(service) = server
                .serve(IntoTransport::<rmcp::RoleServer, _, _>::into_transport(server_side))
                .await
            else {
                return;
            };
            let _ = service.waiting().await;
        });

        Self { client: ().serve(client_side).await.expect("client handshake") }
    }

    async fn example() -> Self {
        Self::connect(example_root()).await
    }

    /// Call a tool and unwrap the structured payload, failing loudly on a tool error —
    /// an error arrives as a successful response with `is_error`, which is exactly the
    /// shape a test silently passes over.
    async fn call(&self, name: &'static str, args: Value) -> Value {
        let result = self.raw(name, args).await;
        assert!(
            result.is_error != Some(true),
            "`{name}` returned an error: {:?}",
            text_of(&result)
        );
        result
            .structured_content
            .unwrap_or_else(|| panic!("`{name}` returned no structured output"))
    }

    /// Call a tool expecting refusal, and return the message the agent would read.
    async fn refuse(&self, name: &'static str, args: Value) -> String {
        let result = self.raw(name, args).await;
        assert_eq!(result.is_error, Some(true), "`{name}` was expected to refuse");
        text_of(&result)
    }

    async fn raw(&self, name: &'static str, args: Value) -> rmcp::model::CallToolResult {
        let params = CallToolRequestParams::new(name)
            .with_arguments(args.as_object().expect("arguments are an object").clone());
        self.client.call_tool(params).await.expect("transport")
    }
}

fn text_of(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

// ------------------------------------------------------------ the surface itself

#[tokio::test]
async fn every_tool_is_advertised_with_a_description_and_a_schema() {
    let agent = Agent::example().await;
    let tools = agent.client.list_all_tools().await.expect("list tools");

    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    for expected in [
        "describe_world",
        "world_at",
        "get_entity",
        "query_entities",
        "timeline",
        "territory_at",
        "lineage",
        "check_consistency",
        "search",
        "resolve_date",
        "list_notes",
        "read_note",
        "list_scenes",
        "read_scene",
        "iceberg",
        "check_changes",
        "propose_changes",
        "list_proposals",
    ] {
        assert!(names.contains(&expected), "missing `{expected}`; have {names:?}");
    }

    for tool in &tools {
        assert!(tool.description.is_some(), "`{}` has no description", tool.name);
        assert_eq!(
            tool.input_schema.get("type").and_then(Value::as_str),
            Some("object"),
            "`{}` has no object input schema",
            tool.name
        );
    }
}

/// The one tool that must never exist. An agent that can accept its own proposals has
/// write access to the writer's world, whatever the queue in front of it says.
#[tokio::test]
async fn nothing_on_the_surface_can_accept_a_proposal() {
    let agent = Agent::example().await;
    let tools = agent.client.list_all_tools().await.unwrap();

    for tool in &tools {
        let name = tool.name.as_ref();
        assert!(
            !["accept_proposal", "decide_proposal", "apply_changes", "write_entity"]
                .contains(&name),
            "`{name}` would let an agent write straight to canon"
        );
    }

    let writers: Vec<&str> = tools
        .iter()
        .filter(|t| t.annotations.as_ref().and_then(|a| a.read_only_hint) != Some(true))
        .map(|t| t.name.as_ref())
        .collect();
    assert_eq!(writers, ["propose_changes"], "only the queue may be written to");
}

#[tokio::test]
async fn the_server_tells_a_cold_agent_how_to_read_this_world() {
    let agent = Agent::example().await;
    let info = agent.client.peer_info().expect("server info");
    let instructions = info.instructions.as_deref().unwrap_or_default();

    assert!(instructions.contains("describe_world"), "the first move is named");
    assert!(instructions.contains("maybe"), "uncertainty is explained before it is met");
    assert!(instructions.contains("propose_changes"), "the write path is named");
}

// ------------------------------------------------------------ orientation

#[tokio::test]
async fn describe_world_carries_everything_a_date_cannot_be_guessed_without() {
    let agent = Agent::example().await;
    let out = agent.call("describe_world", json!({})).await;

    assert_eq!(out["name"], "The Vashen Reckoning");
    assert_eq!(out["calendar"]["days_in_year"], 360, "twelve thirty-day months");
    assert_eq!(out["calendar"]["months"][0]["name"], "Frostwane");
    assert_eq!(out["calendar"]["generation_years"], 30);
    assert_eq!(out["fuzz"]["written_to_the_year"], 730, "a `~` on a year means ±2 years here");

    let syntax = out["date_syntax"].as_array().unwrap();
    assert!(syntax.iter().any(|s| s["form"] == "0812~"));
    assert!(syntax.iter().any(|s| s["form"] == "?"));

    // The vocabulary that already exists, so an agent extends it instead of forking it.
    let attrs: Vec<&str> =
        out["attributes"].as_array().unwrap().iter().map(|a| a["attr"].as_str().unwrap()).collect();
    assert!(attrs.contains(&"owner"), "{attrs:?}");
    assert!(attrs.contains(&"capital"), "{attrs:?}");

    let owner = out["attributes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["attr"] == "owner")
        .expect("owner attribute");
    assert!(owner["examples"].as_array().unwrap().iter().any(|v| v == "pol_vashen"));

    assert_eq!(out["possible_findings"], 1, "the world ships one open question");
    assert_eq!(out["definite_findings"], 0);
    assert_eq!(out["pending_proposals"], 2);
}

// ------------------------------------------------------------ reading

#[tokio::test]
async fn the_snapshot_at_the_siege_reports_doubt_rather_than_resolving_it() {
    let agent = Agent::example().await;
    let out = agent.call("world_at", json!({ "date": "@evt_siege_of_marrow" })).await;

    let vale = out["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "ter_vale_of_corrath")
        .expect("the Vale is on the map at the siege");

    let claims: Vec<&Value> =
        vale["facts"].as_array().unwrap().iter().filter(|f| f["attr"] == "owner").collect();
    assert_eq!(claims.len(), 2, "both claims are live: {claims:#?}");
    assert!(
        claims.iter().all(|c| c["certainty"] == "maybe"),
        "neither claim is settled, and the server must not pick one: {claims:#?}"
    );

    // The change-point bracket is what makes scrubbing cheap, and it is on the payload.
    assert!(out["unchanged_from"].is_i64());
    assert!(out["unchanged_until"].is_i64());
}

#[tokio::test]
async fn a_record_carries_what_points_at_it_which_no_file_states() {
    let agent = Agent::example().await;
    let out = agent.call("get_entity", json!({ "id": "act_aldric_vane" })).await;

    assert_eq!(out["name"], "Aldric Vane");
    assert_eq!(out["primitive"], "actor");
    assert!(out["body"].as_str().unwrap().contains("Fourth of his name"), "prose comes too");

    let appears: Vec<&str> =
        out["appears_in"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap()).collect();
    assert!(appears.contains(&"evt_siege_of_marrow"), "{appears:?}");

    let parents: Vec<&str> =
        out["parents"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert!(parents.contains(&"Maren Vane"), "parents arrive named, not as bare ids: {parents:?}");

    // Every fact carries the window it holds over, as written and as resolved.
    let title = out["facts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["attr"] == "title")
        .expect("a title fact");
    assert!(title["from"]["expr"].as_str().is_some_and(|e| !e.is_empty()));
    assert!(title["from"]["label"].as_str().is_some_and(|l| l != "unknown"));
}

#[tokio::test]
async fn asking_for_a_record_at_a_date_says_whether_it_existed_then() {
    let agent = Agent::example().await;

    let during = agent.call("get_entity", json!({ "id": "place_marrow", "at": "0812" })).await;
    assert_eq!(during["at"]["existence"], "yes");
    let population = during["at"]["facts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["attr"] == "population")
        .expect("population at 0812");
    assert!(population["window"].as_str().is_some_and(|w| !w.is_empty()));

    // Before it was founded there is no snapshot at all — the map would draw nothing.
    let before = agent.call("get_entity", json!({ "id": "place_marrow", "at": "0300" })).await;
    assert!(before["at"].is_null(), "a record that did not exist has no state at that date");
}

#[tokio::test]
async fn queries_filter_on_what_a_writer_would_actually_ask_for() {
    let agent = Agent::example().await;

    let polities = agent.call("query_entities", json!({ "primitive": "polity" })).await;
    assert_eq!(polities["matched"], 2, "{:#?}", polities["items"]);

    let mapped = agent.call("query_entities", json!({ "on_map": true })).await;
    assert!(mapped["matched"].as_u64().unwrap() >= 4);

    let named = agent.call("query_entities", json!({ "name_contains": "vane" })).await;
    let names: Vec<&str> =
        named["items"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"Aldric Vane"), "{names:?}");

    // Marrow is founded around 0602, so it is absent from a snapshot three centuries early.
    let early =
        agent.call("query_entities", json!({ "alive_at": "0300", "primitive": "place" })).await;
    let ids: Vec<&str> =
        early["items"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap()).collect();
    assert!(!ids.contains(&"place_marrow"), "{ids:?}");
}

/// A year given as a window bound means the whole year, not its first instant.
///
/// Collapsing both ends to the nominal day makes the most obvious query anyone could
/// type — "what happened in 806" — return nothing at all, because a year's nominal day
/// is 1 Frostwane and no event sits exactly there.
#[tokio::test]
async fn a_year_as_a_window_bound_means_the_whole_year() {
    let agent = Agent::example().await;
    let out = agent.call("timeline", json!({ "from": "0806", "to": "0806" })).await;

    let ids: Vec<&str> =
        out["items"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["evt_oath_of_vashen"], "the oath is dated 0806-02-14, mid-year");

    // And the year before it is genuinely empty, so the widening is not swallowing
    // everything in sight.
    let quiet = agent.call("timeline", json!({ "from": "0805", "to": "0805" })).await;
    assert_eq!(quiet["matched"], 0);
}

#[tokio::test]
async fn the_timeline_filters_on_who_was_there() {
    let agent = Agent::example().await;

    let involving = agent.call("timeline", json!({ "involving": "act_aldric_vane" })).await;
    let ids: Vec<&str> =
        involving["items"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"evt_siege_of_marrow"), "{ids:?}");
    assert!(!ids.contains(&"evt_founding_of_corrath"), "{ids:?}");

    // A location counts as being involved, which is how "everything at Marrow" works.
    let at_marrow = agent.call("timeline", json!({ "involving": "place_marrow" })).await;
    assert!(at_marrow["matched"].as_u64().unwrap() >= 2);

    let treaties = agent.call("timeline", json!({ "kind": "treaty" })).await;
    assert_eq!(treaties["matched"], 1);
}

#[tokio::test]
async fn territory_is_geojson_shaped_but_says_it_is_not_geographic() {
    let agent = Agent::example().await;
    let out = agent.call("territory_at", json!({ "date": "@evt_siege_of_marrow" })).await;

    assert_eq!(out["type"], "FeatureCollection");
    assert_eq!(out["coordinate_space"], "normalized-image");
    assert!(
        out["note"].as_str().unwrap().contains("southward"),
        "the y-axis flip is stated, not left to be discovered"
    );

    let vale = out["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["properties"]["id"] == "ter_vale_of_corrath")
        .expect("the Vale has geometry");

    assert_eq!(vale["geometry"]["type"], "Polygon");
    let ring = vale["geometry"]["coordinates"][0].as_array().unwrap();
    assert_eq!(ring.first(), ring.last(), "GeoJSON rings close; world files do not have to");
}

/// The thesis, on the agent surface: the map is a projection of the timeline, and a
/// border nobody dated is reported as contested rather than assigned to somebody.
#[tokio::test]
async fn a_contested_border_arrives_contested_and_not_decided() {
    let agent = Agent::example().await;

    let claims_at = async |date: &'static str| {
        let out = agent.call("territory_at", json!({ "date": date })).await;
        out["features"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["properties"]["id"] == "ter_vale_of_corrath")
            .expect("the Vale")["properties"]["claims"]
            .as_array()
            .unwrap()
            .clone()
    };

    let settled = claims_at("0800").await;
    assert_eq!(settled.len(), 1, "long before the siege, one owner and no doubt");
    assert_eq!(settled[0]["owner"], "pol_corrath");
    assert_eq!(settled[0]["certainty"], "yes");
    assert_eq!(settled[0]["color"], "#B07A2B", "the owner's colour is resolved at that date too");

    let contested = claims_at("@evt_siege_of_marrow").await;
    assert_eq!(contested.len(), 2, "the siege is dated to a month, so both claims are live");
    assert!(
        contested.iter().all(|c| c["certainty"] == "maybe"),
        "neither is settled, and the server must not pick: {contested:#?}"
    );
}

#[tokio::test]
async fn lineage_walks_both_directions_from_parentage_edges() {
    let agent = Agent::example().await;
    let out = agent.call("lineage", json!({ "id": "act_aldric_vane" })).await;

    let ancestors: Vec<&str> =
        out["ancestors"].as_array().unwrap().iter().map(|k| k["name"].as_str().unwrap()).collect();
    assert!(ancestors.contains(&"Maren Vane"), "{ancestors:?}");
    assert_eq!(out["ancestors"][0]["generation"], 1);
    assert!(out["ancestors"][0]["lifespan"].as_str().is_some_and(|s| !s.is_empty()));

    let from_parent = agent.call("lineage", json!({ "id": "act_maren_vane" })).await;
    let children: Vec<&str> = from_parent["descendants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["name"].as_str().unwrap())
        .collect();
    assert!(children.contains(&"Aldric Vane"), "{children:?}");
}

#[tokio::test]
async fn the_check_separates_the_open_question_from_an_actual_contradiction() {
    let agent = Agent::example().await;
    let out = agent.call("check_consistency", json!({})).await;

    assert_eq!(out["definite"], 0, "{:#?}", out["findings"]);

    // The world's one deliberate uncertainty — was Aldric alive at the siege? — now
    // surfaces from two different places, and both are worth having. The event record
    // lists him as a participant; chapter twelve names him on the page. Remove him from
    // the participants and the prose finding still stands, which is the point.
    assert_eq!(out["possible"], 2, "{:#?}", out["findings"]);
    let rules: Vec<&str> =
        out["findings"].as_array().unwrap().iter().map(|f| f["rule"].as_str().unwrap()).collect();
    assert!(rules.contains(&"existence-violation"));
    assert!(rules.contains(&"scene-contradiction"));

    let finding = &out["findings"][0];
    assert_eq!(finding["certainty"], "possible");
    assert!(finding["message"].as_str().unwrap().contains("Aldric Vane"));
    assert!(finding["at_label"].as_str().is_some_and(|l| !l.is_empty()), "findings carry a date");
    assert!(
        finding["sources"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| !s.as_str().unwrap().starts_with('/')),
        "paths are relative to the world, not the writer's home directory"
    );

    let filtered = agent.call("check_consistency", json!({ "certainty": "definite" })).await;
    assert_eq!(filtered["findings"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn search_ranks_the_thing_above_the_mentions_of_it() {
    let agent = Agent::example().await;
    let out = agent.call("search", json!({ "text": "marrow" })).await;

    assert_eq!(out["items"][0]["id"], "place_marrow");
    assert_eq!(out["items"][0]["matched"], "name");
    assert!(out["items"].as_array().unwrap().len() > 1);
}

#[tokio::test]
async fn a_date_can_be_checked_before_it_is_written_into_a_proposal() {
    let agent = Agent::example().await;

    // Asserted against the anchor's own resolution rather than a rendered year, since
    // this calendar's era numbering is the world's business and not this tool's.
    let siege = agent.call("resolve_date", json!({ "date": "@evt_siege_of_marrow" })).await;
    let after = agent.call("resolve_date", json!({ "date": "@evt_siege_of_marrow+1y" })).await;
    assert_eq!(
        after["day"].as_i64().unwrap() - siege["day"].as_i64().unwrap(),
        360,
        "one year is twelve thirty-day months here, not 365 days"
    );
    assert!(after["label"].as_str().is_some_and(|l| l.contains("Verdant")), "{after:#?}");

    // The siege is written `0812-04~`, so it stays uncertain a month either way, and an
    // offset from it inherits exactly that doubt rather than becoming exact.
    assert_eq!(after["exact"], false);
    assert_eq!(
        after["latest"].as_i64().unwrap() - after["earliest"].as_i64().unwrap(),
        siege["latest"].as_i64().unwrap() - siege["earliest"].as_i64().unwrap()
    );

    let vague = agent.call("resolve_date", json!({ "date": "0812~" })).await;
    assert_eq!(vague["exact"], false);
    assert!(
        vague["latest"].as_i64().unwrap() - vague["earliest"].as_i64().unwrap() > 700,
        "a `~` on a year widens by this world's two-year fuzz"
    );
}

// ------------------------------------------------------------ refusals

#[tokio::test]
async fn an_unknown_id_comes_back_with_candidates_not_an_empty_result() {
    let agent = Agent::example().await;
    let message = agent.refuse("get_entity", json!({ "id": "act_aldric" })).await;

    assert!(message.contains("act_aldric_vane"), "the near miss is offered: {message}");
}

#[tokio::test]
async fn a_date_that_cannot_be_placed_is_refused_rather_than_guessed() {
    let agent = Agent::example().await;

    let bad = agent.refuse("world_at", json!({ "date": "0812-99-99" })).await;
    assert!(bad.contains("bad date"), "{bad}");

    let unplaceable = agent.refuse("world_at", json!({ "date": "?" })).await;
    assert!(unplaceable.contains("does not resolve"), "{unplaceable}");

    let backwards = agent.refuse("timeline", json!({ "from": "0812", "to": "0500" })).await;
    assert!(backwards.contains("empty"), "{backwards}");
}

// ------------------------------------------------------------ notes

#[tokio::test]
async fn notes_are_listed_and_readable_and_the_folder_is_the_boundary() {
    let agent = Agent::example().await;

    let listed = agent.call("list_notes", json!({})).await;
    let paths: Vec<&str> =
        listed["items"].as_array().unwrap().iter().map(|n| n["path"].as_str().unwrap()).collect();
    assert!(paths.contains(&"notes/houses-and-holdings.md"), "{paths:?}");
    assert!(listed["items"][0]["first_line"].as_str().is_some_and(|l| !l.is_empty()));

    let note = agent.call("read_note", json!({ "path": "notes/houses-and-holdings.md" })).await;
    assert!(note["text"].as_str().unwrap().contains("House Ferrow"));
    assert_eq!(note["truncated"], false);

    // The same file without the folder prefix, since a model will shorten it.
    let short = agent.call("read_note", json!({ "path": "houses-and-holdings.md" })).await;
    assert_eq!(short["path"], note["path"]);
}

#[tokio::test]
async fn a_path_that_climbs_out_of_the_notes_folder_is_refused() {
    let agent = Agent::example().await;

    for escape in ["../world.yaml", "../../../etc/hosts", "notes/../world.yaml"] {
        let message = agent.refuse("read_note", json!({ "path": escape })).await;
        assert!(
            message.contains("outside") || message.contains("no note at"),
            "`{escape}` was not refused clearly: {message}"
        );
    }

    let absolute = agent.refuse("read_note", json!({ "path": "/etc/hosts" })).await;
    assert!(absolute.contains("absolute"), "{absolute}");
}

// ------------------------------------------------------------ writing

#[tokio::test]
async fn a_dry_run_shows_what_a_change_would_settle_without_filing_anything() {
    let root = scratch_world("dry-run");
    let agent = Agent::connect(root.clone()).await;

    let out = agent
        .call(
            "check_changes",
            json!({
                "changes": [{
                    "op": "set_existence",
                    "entity": "act_aldric_vane",
                    "from": "0774",
                    "to": "@evt_siege_of_marrow+1y"
                }]
            }),
        )
        .await;

    assert_eq!(out["resolves"].as_array().unwrap().len(), 1, "{out:#?}");
    assert_eq!(out["introduces"].as_array().unwrap().len(), 0);
    assert_eq!(out["breaks_something"], false);
    assert_eq!(out["possible_after"], 0, "the open question would close");
    assert_eq!(out["files"][0]["path"], "entities/actors/aldric-vane.md");

    // Nothing was filed and nothing was written.
    let queue = agent.call("list_proposals", json!({})).await;
    assert_eq!(queue["matched"], 2, "the queue is untouched");
    assert_eq!(fs::read_dir(root.join("proposals")).unwrap().count(), 2);
}

#[tokio::test]
async fn a_plausible_but_wrong_change_is_caught_before_it_is_ever_filed() {
    let agent = Agent::example().await;

    // Reads perfectly well, and is wrong twice: Marrow does not exist in 0500, and
    // Vashen already has a capital across those years.
    let out = agent
        .call(
            "check_changes",
            json!({
                "changes": [{
                    "op": "add_fact",
                    "entity": "pol_vashen",
                    "attr": "capital",
                    "value": "place_marrow",
                    "from": "0500",
                    "to": "0600"
                }]
            }),
        )
        .await;

    assert_eq!(out["breaks_something"], true);
    let rules: Vec<&str> =
        out["introduces"].as_array().unwrap().iter().map(|f| f["rule"].as_str().unwrap()).collect();
    assert!(rules.contains(&"anachronistic-fact"), "{rules:?}");
    assert!(rules.contains(&"conflicting-facts"), "{rules:?}");
}

#[tokio::test]
async fn filing_a_proposal_writes_to_the_queue_and_nowhere_near_canon() {
    let root = scratch_world("propose");
    let agent = Agent::connect(root.clone()).await;

    let before = fs::read_to_string(root.join("entities/actors/aldric-vane.md")).unwrap();

    let out = agent
        .call(
            "propose_changes",
            json!({
                "title": "House Ferrow holds Greyford, and has since before Corrath",
                "note": "From notes/houses-and-holdings.md — the third house, never recorded.",
                "author": "test-agent",
                "changes": [{
                    "op": "create_entity",
                    "id": "place_greyford",
                    "name": "Greyford",
                    "type": "city",
                    "existence_from": "0480~",
                    "facts": [
                        { "attr": "population", "value": 4200, "from": "0800" }
                    ]
                }]
            }),
        )
        .await;

    assert_eq!(out["status"], "pending");
    assert_eq!(
        out["id"], "prp_house_ferrow_holds_greyford_and_has",
        "the id is readable, not a counter"
    );
    assert!(out["next_step"].as_str().unwrap().contains("writer"));
    assert_eq!(out["impact"]["files"][0]["path"], "entities/places/greyford.md");
    assert_eq!(out["impact"]["files"][0]["is_new"], true);

    // The proposal is on disk...
    let filed = root.join("proposals/prp_house_ferrow_holds_greyford_and_has.yaml");
    assert!(filed.is_file(), "the proposal was written to the queue");
    let yaml = fs::read_to_string(&filed).unwrap();
    assert!(yaml.contains("status: pending"));
    assert!(yaml.contains("op: create_entity"));

    // ...and the world is not.
    assert!(!root.join("entities/places/greyford.md").exists(), "canon was not touched");
    assert_eq!(fs::read_to_string(root.join("entities/actors/aldric-vane.md")).unwrap(), before);

    let queue = agent.call("list_proposals", json!({})).await;
    assert_eq!(queue["matched"], 3);
}

#[tokio::test]
async fn numbers_written_as_numbers_stay_numbers() {
    let root = scratch_world("types");
    let agent = Agent::connect(root.clone()).await;

    agent
        .call(
            "propose_changes",
            json!({
                "title": "Greyford population",
                "changes": [{
                    "op": "create_entity",
                    "id": "place_greyford",
                    "name": "Greyford",
                    "type": "city",
                    "facts": [{ "attr": "population", "value": 4200 }]
                }]
            }),
        )
        .await;

    let yaml = fs::read_to_string(root.join("proposals/prp_greyford_population.yaml")).unwrap();
    assert!(
        yaml.contains("value: 4200"),
        "a population quoted as text sorts as text forever: {yaml}"
    );
}

#[tokio::test]
async fn two_proposals_with_the_same_title_do_not_overwrite_each_other() {
    let root = scratch_world("collide");
    let agent = Agent::connect(root.clone()).await;

    let body = |id: &str| {
        json!({
            "title": "Add a holding",
            "changes": [{ "op": "create_entity", "id": id, "name": "Holding", "type": "city" }]
        })
    };

    let first = agent.call("propose_changes", body("place_one")).await;
    let second = agent.call("propose_changes", body("place_two")).await;

    assert_eq!(first["id"], "prp_add_a_holding");
    assert_eq!(second["id"], "prp_add_a_holding_2");
    assert_eq!(fs::read_dir(root.join("proposals")).unwrap().count(), 4);
}

#[tokio::test]
async fn a_change_that_cannot_be_applied_never_reaches_the_queue() {
    let root = scratch_world("stale");
    let agent = Agent::connect(root.clone()).await;

    let message = agent
        .refuse(
            "propose_changes",
            json!({
                "title": "Move an event that is not there",
                "changes": [{ "op": "set_event_date", "event": "evt_imaginary", "date": "0900" }]
            }),
        )
        .await;

    assert!(message.contains("evt_imaginary"), "{message}");
    assert_eq!(
        fs::read_dir(root.join("proposals")).unwrap().count(),
        2,
        "a proposal that cannot even be simulated is a bug report, not a suggestion"
    );
}

#[tokio::test]
async fn a_malformed_date_is_named_precisely_enough_to_fix() {
    let agent = Agent::example().await;

    let message = agent
        .refuse(
            "check_changes",
            json!({
                "changes": [{
                    "op": "create_event",
                    "id": "evt_x",
                    "name": "Something",
                    "date": "0812~~"
                }]
            }),
        )
        .await;

    assert!(message.contains("date"), "the field is named: {message}");
    assert!(message.contains("0812~~"), "the offending value is quoted back: {message}");
}

#[tokio::test]
async fn an_empty_proposal_is_refused_because_it_asks_the_writer_nothing() {
    let agent = Agent::example().await;
    let message = agent.refuse("propose_changes", json!({ "title": "…", "changes": [] })).await;
    assert!(message.contains("nothing"), "{message}");
}

// ------------------------------------------------------------ staying honest

#[tokio::test]
async fn the_server_notices_the_world_changing_under_it() {
    let root = scratch_world("reload");
    let agent = Agent::connect(root.clone()).await;

    assert_eq!(agent.call("describe_world", json!({})).await["entities"], 11);

    // The writer adds a record in their own editor while the agent is connected.
    fs::write(
        root.join("entities/places/greyford.md"),
        "---\nid: place_greyford\nname: Greyford\ntype: city\n---\n\nUpriver on the Silt.\n",
    )
    .unwrap();

    let after = agent.call("describe_world", json!({})).await;
    assert_eq!(after["entities"], 12, "a stale world would still answer 11");
    assert_eq!(after["reloads"], 1);

    let found = agent.call("get_entity", json!({ "id": "place_greyford" })).await;
    assert_eq!(found["name"], "Greyford");
}

#[tokio::test]
async fn the_ground_under_a_settlement_is_reported_without_a_date() {
    let agent = Agent::example().await;
    let out = agent.call("describe_place", json!({ "entity": "place_corrath_city" })).await;

    assert_eq!(out["entity"], "place_corrath_city");
    assert_eq!(out["is_land"], true, "a city in the sea would be a placement bug");
    assert_eq!(out["biome"], "temperate forest");
    assert_eq!(out["on_river"], true, "Corrath sits on the Silt");

    // The point it answered about is the record's own marker, and the nearest named
    // record to a city is itself — which is how a reader checks the answer is not adrift.
    assert_eq!(out["at"], json!([0.25, 0.45]));
    assert_eq!(out["near"][0]["id"], "place_corrath_city");
    assert_eq!(out["near"][0]["distance"], 0.0);
}

#[tokio::test]
async fn the_rain_shadow_is_visible_through_the_tool_surface() {
    let agent = Agent::example().await;
    let rain = async |id| {
        agent.call("describe_place", json!({ "entity": id })).await["rainfall"].as_f64().unwrap()
    };

    // West of the Marrow Wall, in the gap, and behind it. This is the whole reason the
    // Vale is worth taking, and it comes out of the climate rather than out of a fact.
    let (vale, gap, steppe) = (
        rain("place_corrath_city").await,
        rain("place_marrow").await,
        rain("place_vashen_seat").await,
    );
    assert!(vale > gap, "the Vale ({vale}) should be wetter than Marrow ({gap})");
    assert!(gap > steppe, "Marrow ({gap}) should be wetter than the heartland ({steppe})");
}

#[tokio::test]
async fn a_place_needs_either_a_record_or_a_pair_of_coordinates() {
    let agent = Agent::example().await;

    let both = agent.refuse("describe_place", json!({ "entity": "place_marrow", "x": 0.5 })).await;
    assert!(both.contains("either"), "unhelpful: {both}");

    // A polity has no marker; saying so beats returning the terrain at [0, 0].
    let no_marker = agent.refuse("describe_place", json!({ "entity": "pol_corrath" })).await;
    assert!(no_marker.contains("marker"), "unhelpful: {no_marker}");
}

#[tokio::test]
async fn sites_can_be_filtered_by_the_ground_and_ranked_by_distance() {
    let agent = Agent::example().await;
    let out = agent
        .call("find_sites", json!({ "on_river": true, "near": "place_marrow", "within": 0.2 }))
        .await;

    let items = out["items"].as_array().expect("a list");
    assert!(!items.is_empty(), "the Silt runs past Marrow, so something is upriver of it");
    assert!(out["matched"].as_u64().unwrap() >= items.len() as u64);

    for site in items {
        assert_eq!(site["on_river"], true, "the filter is a filter, not a preference");
        assert!(site["from_anchor"].as_f64().unwrap() <= 0.2, "`within` is a bound, not a hint");
    }

    // Nearest first, so "upriver from Marrow" can be answered by reading down the list —
    // and the distance is in the payload, so the ranking does not have to be taken on trust.
    let reach = |s: &Value| s["from_anchor"].as_f64().unwrap();
    assert!(
        items.windows(2).all(|w| reach(&w[0]) <= reach(&w[1]) + 1e-9),
        "candidates should come back nearest first"
    );
}

#[tokio::test]
async fn an_impossible_site_returns_nothing_rather_than_the_nearest_thing() {
    let agent = Agent::example().await;
    let out = agent.call("find_sites", json!({ "biome": "rainforest", "coastal": true })).await;

    assert_eq!(out["matched"], 0, "this world has no tropics; inventing one would be worse");
    assert!(out["items"].as_array().unwrap().is_empty());

    // With no anchor there is nothing to be near, and the field is absent rather than zero.
    let anywhere = agent.call("find_sites", json!({ "coastal": true, "limit": 1 })).await;
    assert!(anywhere["items"][0].get("from_anchor").is_none());
}

#[tokio::test]
async fn orientation_carries_the_ground_and_says_it_is_not_canon() {
    let agent = Agent::example().await;
    let terrain = &agent.call("describe_world", json!({})).await["terrain"];

    assert_eq!(terrain["source"], "map/vashen.png");
    assert_eq!(terrain["source_pixels"], json!([2000, 1400]));
    assert!(terrain["rivers"].as_u64().unwrap() > 0);
    assert!(
        terrain["note"].as_str().unwrap().contains("not canon"),
        "an agent that thinks it can propose against terrain will waste a proposal"
    );
}

/// Omission must never be destructive.
///
/// Aldric's birth is recorded and his death is the world's one open question. An agent
/// correcting the death date will naturally send only `to`, and if that cleared `from`
/// it would silently delete a date nobody asked about — the exact shape of mistake the
/// review queue exists to make visible, arriving in a form the diff makes look
/// deliberate.
#[tokio::test]
async fn omitting_one_end_of_set_existence_does_not_erase_the_other() {
    let agent = Agent::example().await;

    let out = agent
        .call(
            "check_changes",
            json!({
                "changes": [{
                    "op": "set_existence",
                    "entity": "act_aldric_vane",
                    "to": "@evt_siege_of_marrow+1y"
                }]
            }),
        )
        .await;

    // Aldric's existence is one inline line, `{ from: "0771-06-12", to: "0811~" }`.
    // Setting only `to` rewrites that one line. If omission cleared `from`, the birth
    // date would be gone from it — and because the whole span is one line, the way to
    // see that is that the change still settles the open question rather than opening
    // new ones about a man with no birth date.
    assert_eq!(out["files"][0]["changed_lines"], 1, "{out:#?}");
    assert_eq!(out["resolves"].as_array().unwrap().len(), 1, "the death date is settled");
    assert_eq!(out["breaks_something"], false, "{out:#?}");
    assert_eq!(
        out["possible_after"], 0,
        "clearing the birth date would leave the parentage check unable to conclude: {out:#?}"
    );
}

#[tokio::test]
async fn a_question_mark_clears_an_existence_end() {
    let agent = Agent::example().await;

    let out = agent
        .call(
            "check_changes",
            json!({
                "changes": [{
                    "op": "set_existence",
                    "entity": "act_aldric_vane",
                    "from": "?"
                }]
            }),
        )
        .await;

    // Clearing a birth date is legal and loses the parentage check its precision fed,
    // so the call must succeed rather than error.
    assert!(out.get("error").is_none(), "clearing an end should be expressible: {out:#?}");
}

// ------------------------------------------------------------ the manuscript

/// The book comes back in reading order, which is the manuscript's order and not the
/// calendar's. Chapter one holds a flashback, so the two genuinely disagree — and an
/// agent that sorted these by date would narrate the book wrong.
#[tokio::test]
async fn scenes_come_back_in_the_order_they_are_read() {
    let agent = Agent::example().await;
    let out = agent.call("list_scenes", json!({})).await;

    let items = out["items"].as_array().expect("items");
    assert_eq!(items.len(), 3);

    let ids: Vec<&str> = items.iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["scn_gate_at_dusk", "scn_word_from_the_vale", "scn_the_breach"]);
    assert_eq!(items[0]["reading_order"], json!(1), "counted from one, for a person");

    let second = items[1]["nominal"].as_i64().unwrap();
    let first = items[0]["nominal"].as_i64().unwrap();
    assert!(second < first, "the second scene read is set eight years before the first");

    assert_eq!(items[2]["prose"], json!("ch12-the-siege.md#the-breach"));
    assert!(items[2]["names_records"].as_u64().unwrap() > 0);
}

/// A passage arrives with the records it names already resolved, so an agent never has
/// to guess which proper nouns in the prose are canon and which are furniture.
#[tokio::test]
async fn a_scene_arrives_with_the_records_its_prose_names() {
    let agent = Agent::example().await;
    let out = agent.call("read_scene", json!({ "scene": "scn_the_breach" })).await;

    assert_eq!(out["file"], json!("ch12-the-siege.md"));
    assert_eq!(out["heading"], json!("The breach"));
    assert!(out["text"].as_str().unwrap().contains("the wall of Marrow opened"));
    assert!(
        !out["text"].as_str().unwrap().contains("Twelve — The Siege"),
        "the anchor narrows to its own section rather than the whole chapter"
    );

    let names = out["names"].as_array().expect("names");
    let ids: Vec<&str> = names.iter().map(|n| n["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"place_marrow"));
    assert!(ids.contains(&"act_aldric_vane"));

    for name in names {
        assert!(
            !name["first_seen"].as_str().unwrap_or_default().is_empty(),
            "every count carries the sentence behind it: {name:?}"
        );
    }
}

#[tokio::test]
async fn a_scene_that_does_not_exist_says_how_to_find_the_ones_that_do() {
    let agent = Agent::example().await;
    let message = agent.refuse("read_scene", json!({ "scene": "scn_nowhere" })).await;
    assert!(message.contains("list_scenes"), "got: {message}");
}

/// The iceberg's headline number, sorted the way the methodology says to read it.
#[tokio::test]
async fn the_iceberg_reports_what_reaches_the_page_and_says_what_it_measured() {
    let agent = Agent::example().await;
    let out = agent.call("iceberg", json!({})).await;

    assert_eq!(out["standing"], json!("linked"));
    assert_eq!(out["total"], json!(11));
    assert_eq!(out["surfaced"], json!(8));
    assert_eq!(out["surfaced_percent"], json!(73));

    let records = out["records"].as_array().expect("records");
    assert_eq!(records.len(), 11);

    // The caveat travels on the payload, not only in a skill, because the number is what
    // gets quoted and the number is the part that can mislead.
    let note = out["note"].as_str().unwrap();
    assert!(note.contains("aka"), "the note must explain the commonest cause of a low ratio");

    // Underbuilt first — the order *is* the report's opinion about where the hour goes.
    let standings: Vec<&str> = records.iter().map(|r| r["standing"].as_str().unwrap()).collect();
    let rank = |s: &str| match s {
        "underbuilt" => 0,
        "load-bearing" => 1,
        "overbuilt" => 2,
        _ => 3,
    };
    assert!(
        standings.windows(2).all(|w| rank(w[0]) <= rank(w[1])),
        "records are not sorted underbuilt-first: {standings:?}"
    );
}

/// The rule DESIGN.md §5 deferred, reaching an agent through the tool it already uses.
#[tokio::test]
async fn a_contradiction_found_in_the_prose_reads_like_any_other_finding() {
    let agent = Agent::example().await;
    let out = agent.call("check_consistency", json!({ "rule": "scene-contradiction" })).await;

    let findings = out["findings"].as_array().expect("findings");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["subject"], json!("scn_the_breach"));
    assert_eq!(
        findings[0]["certainty"],
        json!("possible"),
        "his death is `0811~` and the siege is `0812-04~`, so the world permits it"
    );
    assert!(findings[0]["message"].as_str().unwrap().contains("on the page"));
}
