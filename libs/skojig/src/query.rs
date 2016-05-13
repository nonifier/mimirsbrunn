// Copyright © 2016, Canal TP and/or its affiliates. All rights reserved.
//
// This file is part of Navitia,
//     the software to build cool stuff with public transport.
//
// Hope you'll enjoy and contribute to this project,
//     powered by Canal TP (www.canaltp.fr).
// Help us simplify mobility and open public transport:
//     a non ending quest to the responsive locomotion way of traveling!
//
// LICENCE: This program is free software; you can redistribute it
// and/or modify it under the terms of the GNU Affero General Public
// License as published by the Free Software Foundation, either
// version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with this program. If not, see
// <http://www.gnu.org/licenses/>.
//
// Stay tuned using
// twitter @navitia
// IRC #navitia on freenode
// https://groups.google.com/d/forum/navitia
// www.navitia.io
use super::{Args, model};
use regex;
use rs_es;
use rs_es::query::Query as rs_q;
use rs_es::operations::search::SearchResult;
use mimir;
use serde_json;

fn build_rs_client(cnx: &String) -> rs_es::Client {
    let re = regex::Regex::new(r"(?:https?://)?(?P<host>.+?):(?P<port>\d+)/(?P<index>\w+)")
                 .unwrap();
    let cap = re.captures(&cnx).unwrap();
    let host = cap.name("host").unwrap();
    let port = cap.name("port").unwrap().parse::<u32>().unwrap();

    rs_es::Client::new(&host, port)
}

/// takes a ES json blob and build a Place from it
/// it uses the _type field of ES to know which type of the Place enum to fill
fn make_place(doc_type: String, value: Option<Box<serde_json::Value>>) -> Option<mimir::Place> {
    value.and_then(|v| {
        match doc_type.as_ref() {
            "addr" => serde_json::from_value::<mimir::Addr>(*v).ok().and_then(|o| Some(mimir::Place::Addr(o))),
            "street" => serde_json::from_value::<mimir::Street>(*v).ok().and_then(|o| Some(mimir::Place::Street(o))),
            "admin" => serde_json::from_value::<mimir::Admin>(*v).ok().and_then(|o| Some(mimir::Place::Admin(o))),
            _    => {
                warn!("unknown ES return value, _type field = {}", doc_type);
                None
            }
        }
    })
}

fn query(q: &String, cnx: &String) -> Result<Vec<mimir::Place>, rs_es::error::EsError> {
    let sub_query = rs_q::build_bool()
                        .with_should(vec![
                       rs_q::build_term("_type","addr").with_boost(1000).build(),
                       rs_q::build_match("name.prefix", q.to_string())
                              .with_boost(100)
                              .build(),
                       rs_q::build_function_score()
                              .with_boost_mode(rs_es::query::compound::BoostMode::Multiply)
                              .with_boost(30)
                              .with_query(rs_q::build_match_all().build())
			                  .with_function(
                          rs_es::query::functions::Function::build_field_value_factor("weight")
			                                  .with_factor(1)
                                      .with_modifier(rs_es::query::functions::Modifier::Log1p)
			                                                          .build())
                              .build()])
                        .build();
    let filter = rs_q::build_bool()
                     .with_should(vec![rs_q::build_bool()
                                           .with_must_not(rs_q::build_exists("house_number")
                                                              .build())
                                           .build(),
                                       rs_q::build_match("house_number", q.to_string()).build()])
                     .with_must(vec![rs_q::build_match("name.prefix", q.to_string())
             .with_minimum_should_match(rs_es::query::MinimumShouldMatch::from(100f64)).build()])
                     .build();

    let final_query = rs_q::build_bool()
                          .with_must(vec![sub_query])
                          .with_filter(filter)
                          .build();

    let mut client = build_rs_client(cnx);

    let result: SearchResult<serde_json::Value> = try!(client.search_query()
                                                       .with_indexes(&["munin"])
                                                       .with_query(&final_query)
                                                       .send());

    debug!("{} documents found", result.hits.total);

    // for the moment rs-es does not handle enum Document,
    // so we need to convert the ES glob to a Place
    Ok(result.hits
             .hits
             .into_iter()
             .filter_map(|hit| make_place(hit.doc_type, hit.source))
             .collect())
}

fn query_location(_q: &String,
                  _coord: &model::Coord)
                  -> Result<Vec<mimir::Place>, rs_es::error::EsError> {
    panic!("todo!");
}

pub fn autocomplete(q: String,
                    coord: Option<model::Coord>,
                    cnx: &String)
                    -> Result<Vec<mimir::Place>, rs_es::error::EsError> {
    if let Some(ref coord) = coord {
        query_location(&q, coord)
    } else {
        query(&q, cnx)
    }
}