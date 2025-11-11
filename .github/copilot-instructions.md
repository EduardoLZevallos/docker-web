# Testing
- using the naming convention for tests: method_name_state_under_test_expected_behavior. Make sure it's snake_case.
- Should always use GIVEN WHEN THEN structure for clarity
- look to create integration tests to cover end to end functionality when possible.
- look to test functionality, do not mock a bunch of stuff and just test that your mock works.
- always look for redundancy in tests and remove redundant tests.
- look to bring up actual containers and networks with testcontainers to test against real docker api responses.
- do not test against the real docker environment on the host machine, use testcontainers to bring up a controlled test environment.

# Rust
- when adding crates always use cargo add <crate_name>
- backend folder is the rust project

# Overall Project
- don't hardcode config use environment variables and store in a .env file