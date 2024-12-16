# nadeo-api

A library to making using the Nadeo API easy.

## Usage

1. Create a `NadeoCredentials`.
2. Create a `UserAgentDetails`.
3. Create a `NadeoClient` via `NadeoClient::create`.
4. Call methods like `let cotd = client.get_cup_of_the_day()?;`

For lower-level access, see `NadeoApiClient::run_*` methods.

For examples of how to do more complex low-level requests, see `LiveApiClient::get_map_group_leaderboard` for an example.