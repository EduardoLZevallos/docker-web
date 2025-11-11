# Testing
- using the naming convention for unit tests :  MethodName_StateUnderTest_ExpectedBehavior
- Should always use GIVEN WHEN THEN structure for clarity
- look to create integration tests to cover end to end functionality when possible.
- look to test functionality, do not mock a bunch of stuff and just test that your mock works.
- always look for redundancy in tests and remove redundant tests.

# Rust
- when adding crates always use cargo add <crate_name>
- backend folder is the rust project

# Overall Project
- don't hardcode config use environment variables and store in a .env file