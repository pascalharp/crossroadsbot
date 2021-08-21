![Crossroads](https://cdn.discordapp.com/icons/226398442082140160/03fe915815e9dbb6cdd18fe577fc6dd9.webp)

# Crossroads Inn signup bot

# Setup
Following a non config file approach required tokens and paths are passed to the application with
environment variables. To not pollute your environment while developing check out the [.env
file](#mardown-header-.env-file) section below
## Docker Compose setup
Copy *docker-compose.yml.example* to *docker-compose.yml* like so: `cp docker-compose.yml.example
docker-compose.yml`. Then edit *docker-compose.yml* and fill the missing fields with your
discord bot token and your discord guild and role id's. At last start the bot and the postgres
database in detached mode with `docker-compose up -d`.
## Environment variables
### DATABASE\_URL
URL to postgres database.\
Example: *DATABASE\_URL=postgres://username:password@localhost/crossroad*
### DISCORD\_TOKEN
The discord bot token to be used. [Check here](https://discord.com/developers/docs/intro) for more
information.
### APPLICATION\_ID
The application id of the bot. This is required to use buttons
### MAIN\_GUILD\_ID
The main discord guild id the bot will be used on. This is also the discord where role
permissions are taken from
### EMOJI\_GUILD\_ID
The discord guild the bot will load and use custom emojis from.
### ADMIN\_ROLE\_ID
The discord role id for MAIN\_GUILD\_ID that has access to all commands
### SQUADMAKER\_ROLE\_ID
The discord role id for MAIN\_GUILD\_ID that has access selected commands
### RUST\_LOG
Amount of LOG verbosity. Options are: `warn, info, debug`

## .env file
A *.env* file can be placed in the root directory of the project that will be sourced when the
application is started and for all tests.\
Example *./.env* file content:
```
DATABASE_URL=postgres://username:password@localhost/crossroad
DISCORD_TOKEN=AVERYLONGSECRETTOKENTHATSHOULDNEVERBEMADEPUBLIC
MAIN_GUILD_ID=111222333444555666
EMOJI_GUILD_ID=111222333444555666
ADMIN_ROLE_ID=666777888999000111
SQUADMAKER_ROLE_ID=666777888999000111
RUST_LOG=info
```
