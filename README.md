![Crossroads](https://cdn.discordapp.com/icons/226398442082140160/03fe915815e9dbb6cdd18fe577fc6dd9.webp)

# Crossroads Inn signup bot

# Setup
Following a non config file approach required tokens and paths are passed to the application with
environment variables. To not pollute your environment while developing check out the [.env
file](#mardown-header-.env-file) section below

## Environment variables
### DATABASE\_URL
URL to postgres database.\
Example: *DATABASE\_URL=postgres://username:password@localhost/crossroad*
### DISCORD\_TOKEN
The discord bot token to be used. [Check here](https://discord.com/developers/docs/intro) for more
information.

## Environment variables (dev only)
### TEST\_BASE\_URL
Base URL for database tests. The postgres user has to have permissions to create new databases since
every test creates a new databse, runs all migrations on them, executes the tests and finally
removes the databse again.\
Example: *TEST\_BASE\_URL=postgres:://username:password@localhost*

## .env file
A *.env* file can be placed in the rood directory of the project that will be sourced when the
application is started and for all tests.\
Example *./.env* file content:
```
DATABASE_URL=postgres://username:password@localhost/crossroad
TEST_BASE_URL=postgres://username:password@localhost
```
