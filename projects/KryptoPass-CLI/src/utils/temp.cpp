
//#include <SQLiteCpp/SQLiteCpp.h>
//#include <iostream>
//
//#include "include/unicode.h"
//#include "include/logger.h"
//#include "include/main.h"

//UNICODE_ENSURE;
//Console console{ spdlog::level::trace };
//
//try {
//    KryptoPass cli(argc, argv);
//    cli.parser();
//
//    if (!KPL::init()) {
//        QString error = QObject::tr("Fatal error while testing the cryptographic functions.");
//        error.append("\n");
//        error.append(Crypto::errorString());
//        MessageBox::critical(nullptr, QObject::tr("KeePassXC - Error"), error);
//        return EXIT_FAILURE;
//    }
//
//    cli.bootstrap();
//
//    return 0;
//}
//catch (const KPException& e) {
//    console.error("Error in KryptoPass: {}", e.what());
//    return 1;
//}
//catch (const std::exception& e) {
//    console.error("Error in : {}", e.what());
//    return 1;
//}

//return testSha256() && testSha512() && testAes256Cbc() && testAesKdf() && testTwofish() && testSalsa20()
//&& testChaCha20();

// Vertedero de comentarios

//#include "include/database.h"
//Database db;

//CREATE TABLE "metadata" (
//    "version"	TEXT NOT NULL DEFAULT '1.0.0-beta',
//    "kp_min_version"	TEXT NOT NULL DEFAULT '1.0.0'
//);

//CREATE TABLE "profiles" (
//    "id"	INTEGER NOT NULL UNIQUE,
//    "username"	TEXT NOT NULL,
//    "create_date"	INTEGER NOT NULL,
//    "db_path"	TEXT NOT NULL,
//    "db_timestamp"	INTEGER NOT NULL,
//    "db_hash"	TEXT NOT NULL,
//    PRIMARY KEY("id" AUTOINCREMENT)
//);




//catch (const DBException& e) {
//    console.error("An error has occurred in the database object: {}", e.what());
//    return 1;
//}






//int err;
//zip_t* zip = zip_open("C:/Users/gmaizo/Videos/sqlite3a.zip", 0, &err);
//
//if (zip == NULL) {
//    printf("Error al abrir el archivo zip\n");
//    return 1;
//}
//
//zip_int64_t num_entries = zip_get_num_entries(zip, 0);
//
//console.log("{}{}", (char*)u8"Número de entradas en el archivo zip: ", num_entries);
//
//zip_close(zip);


//#include <zip.h>

//Tu fortaleza digital de secretos custodiada por hechicería criptográfica!.🌟🔐
//¡Tu fortaleza digital personal de secretos, custodiada por expertos magos de la criptografía!
//¡Magia criptográfica protegiendo tus secretos digitales!🌟🔐

//KryptoPass cli{ argc, argv };
//return cli.Parser();
//console.log("{}", (char*)u8"Ññ初音ミク🥹✨😎😉😀😣(●'◡'●)");
//static int callback(void* NotUsed, int argc, char** argv, char** azColName) {
//	for (int i = 0; i < argc; i++) {
//		std::cout << azColName[i] << " = " << (argv[i] ? argv[i] : "NULL") << std::endl;
//	}
//	std::cout << std::endl;
//	return 0;
//}
//sqlite3* db;
//int rc{ sqlite3_open("KryptoPass.db3", &db) };
//char* zErrMsg = 0;
//
//if (rc) {
//	console.error("Can't open database: {}\n", sqlite3_errmsg(db));
//	sqlite3_close(db);
//	return 1;
//}
//
//rc = sqlite3_exec(db,
//	"CREATE TABLE IF NOT EXISTS usuarios ("
//	"id INTEGER PRIMARY KEY,"
//	"nombre TEXT NOT NULL,"
//	"apellido TEXT,"
//	"edad INTEGER"
//	");"
//
//	"INSERT INTO usuarios(nombre, apellido, edad)"
//	"VALUES('Juan', 'Pérez', 30), ('María', 'García', 25), ('Pedro', 'López', 40);", NULL, NULL, NULL);
//
//
//if (rc != SQLITE_OK) {
//	console.error("SQL error: {}\n", zErrMsg);
//	sqlite3_free(zErrMsg);
//}
//
//// Execute the SELECT query and print the results
//const char* sql = "SELECT * FROM usuarios;";
//rc = sqlite3_exec(db, sql, callback, 0, &zErrMsg);
//if (rc != SQLITE_OK) {
//	console.error("SQL error: {}\n", zErrMsg);
//	sqlite3_free(zErrMsg);
//	return 1;
//}
//
//sqlite3_close(db);
//return 0;

// Abrir un archivo ZIP en modo escritura

//// Agregar un archivo de texto al archivo ZIP
//const char* textContent = "¡Hola, Mundo!";
// 
//zip_source_t* source = zip_source_buffer(zip, textContent, strlen(textContent), 0);
//zip_file_add(zip, "hello.txt", source, ZIP_FL_OVERWRITE);

//// Cerrar el archivo ZIP
//zip_close(zip);

//std::cout << "¡Archivo ZIP creado exitosamente!" << std::endl;


//Database.kpdb
//│   KryptoPass.db
//│
//├───Auditing
//│   ├───Change History
//│   │       Access Logs.db
//│   │       File Modifications.db
//│   │
//│   └───Version Management
//│           Rollback Capabilities.db
//│           Version.txt
//│
//├───Content Folders
//│   ├───Audios
//│   ├───Binaries
//│   ├───Documents
//│   ├───Images
//│   ├───Notes
//│   ├───Others
//│   └───Videos
//├───Metadata
//│       File List.db
//│       Summary.db
//│
//├───User Assets
//│       Login Data.db
//│       Payment Data.db
//│       Secret Data.db
//│
//└───User Interface
//        Command Line.db

//try {
//	// Compile a SQL query, containing one parameter (index 1)
//	SQLite::Statement query(db, "SELECT * FROM test WHERE size > ?");
//	// Bind the integer value 6 to the first parameter of the SQL query
//	query.bind(1, 6);
//	// Loop to execute the query step by step, to get rows of result
//	while (query.executeStep())
//	{
//		// Demonstrate how to get some typed column value
//		int id = query.getColumn(0);
//		const char* value = query.getColumn(1);
//		int size = query.getColumn(2);
//		std::cout << "row: " << id << ", " << value << ", " << size << std::endl;
//	}
//}
//catch (std::exception& e) {
//	std::cout << "exception: " << e.what() << std::endl;
//}


// Content Folders
//const char* contentFolders = "Content Folders";
//const char* documents = "Documents";
//const char* binaries = "Binaries";
//const char* audios = "Audios";
//const char* videos = "Videos";
//const char* images = "Images";
//const char* others = "Others";
//const char* notes = "Notes";
//
//// Auditing Folders
//const char* auditing = "Auditing";
//const char* changeHistory = "Change History";
//const char* versionManagement = "Version Management";
//
//// Metadata Folder
//const char* metadata = "Metadata";
//
//// Metadata Files
//const char* fileListDB = "File List.db";
//const char* summaryDB = "Summary.db";


//std::cout << "home: " << shell.getPath("home") << std::endl;
//std::cout << "appdata: " << shell.getPath("appdata") << std::endl;
//std::cout << "desktop: " << shell.getPath("desktop") << std::endl;
//std::cout << "documents: " << shell.getPath("documents") << std::endl;
//std::cout << "downloads: " << shell.getPath("downloads") << std::endl;
//std::cout << "music: " << shell.getPath("music") << std::endl;
//std::cout << "pictures: " << shell.getPath("pictures") << std::endl;
//std::cout << "temp: " << shell.getPath("temp") << std::endl;
//std::cout << "videos: " << shell.getPath("videos") << std::endl;
//std::cout << "roaming: " << shell.getPath("roaming") << std::endl;
//std::cout << "local: " << shell.getPath("local") << std::endl;


//home: C:\Users\gmaizo
//appdata : C:\Users\gmaizo\AppData\Roaming
//temp : C:\Users\gmaizo\AppData\Local
//desktop : C:\Users\gmaizo\Desktop
//documents : C:\Users\gmaizo\Documents
//downloads : C:\Users\gmaizo\Downloads
//music : C:\Users\gmaizo\Music
//pictures : C:\Users\gmaizo\Pictures
//videos : C:\Users\gmaizo\Videos

//Temp


//const char* env_p = std::getenv("PATH")

//LOCALAPPDATA

//""

	//db(path, SQLite::OPEN_READWRITE | SQLite::OPEN_CREATE)
	//const std::string& path

//SQLite::Database db(dbFile.string(), SQLite::OPEN_READWRITE | SQLite::OPEN_CREATE);

//try {
//	Shell shell;
//	Console console;
//
//	auto kpdbPath = std::filesystem::path(shell.getPath("roaming")) / "KryptoPass";
//
//	// Verifica si el directorio existe, si no, lo crea
//	if (!std::filesystem::exists(kpdbPath)) {
//		std::filesystem::create_directories(kpdbPath);
//	}
//
//	// Ruta completa del archivo de la base de datos
//	auto dbFile = kpdbPath / "Database.kpdb";
//
//	// Zip Manager
//	int openerr;
//	zip_t* zip = zip_open(dbFile.string().c_str(), ZIP_CREATE | ZIP_TRUNCATE, &openerr);
//
//
//	if (!zip) {
//		zip_error_t error;
//		zip_error_init_with_code(&error, openerr);
//		console.log("{}: cannot open zip archive '{}': {}", "progname", "name", zip_error_strerror(&error));
//		zip_error_fini(&error);
//	}
//
//	// Auditing Folders
//	zip_dir_add(zip, "Auditing", NULL);
//	zip_dir_add(zip, "Auditing/Change History", NULL);
//	zip_dir_add(zip, "Auditing/Version Management", NULL);
//
//	// Content Folders
//	zip_dir_add(zip, "Content Folders", NULL);
//	zip_dir_add(zip, "Content Folders/Audios", NULL);
//	zip_dir_add(zip, "Content Folders/Binaries", NULL);
//	zip_dir_add(zip, "Content Folders/Documents", NULL);
//	zip_dir_add(zip, "Content Folders/Images", NULL);
//	zip_dir_add(zip, "Content Folders/Notes", NULL);
//	zip_dir_add(zip, "Content Folders/Others", NULL);
//	zip_dir_add(zip, "Content Folders/Videos", NULL);
//
//	// Metadata Folders
//	zip_dir_add(zip, "Metadata", NULL);
//	zip_dir_add(zip, "User Assets", NULL);
//	zip_dir_add(zip, "User Interface", NULL);
//
//	std::string config = (std::filesystem::path(shell.getPath("temp")) / shell.uuidv4()).string();
//	SQLite::Database db(config, SQLite::OPEN_READWRITE | SQLite::OPEN_CREATE);
//
//	zip_source* s = zip_source_file(zip, config.c_str(), 0, 0);
//
//	if (s == nullptr) {
//		throw std::runtime_error("Error creating zip source");
//	}
//
//	zip_file_add(zip, "KryptoPass.db", s, ZIP_FL_ENC_UTF_8 | ZIP_FL_OVERWRITE);
//
//
//	zip_close(zip);
//}
//catch (const std::exception& e) {
//	// Manejar la excepción aquí
//	std::cerr << "Error: " << e.what() << std::endl;
//}


//int Database::create(const char* path)
//{
//	Console console;
//	Shell shell;
//
//	int openerr;
//	zip_t* zip = zip_open(path, ZIP_CREATE | ZIP_TRUNCATE, &openerr);
//	
//	if (!zip) {
//		zip_error_t error;
//		zip_error_init_with_code(&error, openerr);
//		console.error("{}: failed to create the database {} : {}", path, EXT_DATABASE_NAME, zip_error_strerror(&error));
//		zip_error_fini(&error);
//	}
//
//	try {
//
//
//		auto kpdb = std::filesystem::path(shell.getPath("temp")) / shell.uuidv4();
//		auto kpdbstd = kpdb.string();
//
//		createDatabase(kpdbstd);
//
//		zip_source* source = zip_source_file(zip, kpdbstd.c_str(), 0, 0);
//
//		if (source == nullptr) {
//			throw std::runtime_error("Error creating zip source");
//		}
//
//		zip_file_add(zip, "KryptoPass.db3", source, ZIP_FL_ENC_UTF_8 | ZIP_FL_OVERWRITE);
//
//		std::filesystem::remove(kpdb);
//	}
//	catch (const std::exception&) {
//
//	}
//
//	zip_close(zip);
//}



//#include <KryptoPassLib.h>
//#include <CLI/CLI.hpp>
//#include <iostream>
//
//#include "settings.h"
//#include "logger.h"
//#include "shell.h"
//#include "main.h"
//
//KryptoPass::KryptoPass(int argc, char** argv, Console console) : app{ (char*)u8"KryptoPass: Your digital fortress of secrets guarded by cryptographic wizardry! 🌟🔐", "KryptoPass" }
//{
//	Settings settings;
//
//	this->argc = argc;
//	this->argv = argv;
//
//	this->console = console;
//	this->settings = settings;
//}
//
//KryptoPass::~KryptoPass()
//{
//	kpl::close();
//}
//
//int KryptoPass::parser()
//{
//	try {
//		app.parse(argc, argv);
//		return this->handled();
//	}
//	catch (const CLI::ParseError& e) {
//		return (app).exit(e);
//	}
//}
//
//int KryptoPass::handled()
//{
//	return 0;
//}

//app.add_option("-f,--file", filename, "A help string");
//int success = kpl::init();
//
//if (!success) {
//	std::cerr << "Error initializing OpenSSL" << std::endl;
//	std::exit(1);
//}

//bool newProfile = true;
//if (newProfile) {
//	Settings settings;
//	std::string username = "Undead34";
//	std::filesystem::path DBPath = shell.getPath("");
//	
//	std::filesystem::path(shell.getPath("roaming")) / "KryptoPass";

//	time_t createDate = std::chrono::system_clock::to_time_t(std::chrono::system_clock::now());
//}

//CREATE TABLE "profiles" (
//    "id"	INTEGER NOT NULL UNIQUE,
//    "username"	TEXT NOT NULL,
//    "create_date"	INTEGER NOT NULL,
//    "db_path"	TEXT NOT NULL,
//    "db_timestamp"	INTEGER NOT NULL,
//    "db_hash"	TEXT NOT NULL,
//    PRIMARY KEY("id" AUTOINCREMENT)
//);


//#include <vector>
//#include <iostream>
//#include <random>
//
//// Alfabetos
//std::vector<std::string> latinAlphabet = { "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z" };
//std::vector<std::string> hiraganaAlphabet = { "あ", "い", "う", "え", "お", "か", "き", "く", "け", "こ", "さ", "し", "す", "せ", "そ", "た", "ち", "つ", "て", "と", "な", "に", "ぬ", "ね", "の", "は", "ひ", "ふ", "へ", "ほ", "ま", "み", "む", "め", "も", "や", "ゆ", "よ", "ら", "り", "る", "れ", "ろ", "わ", "を", "ん" };
//std::vector<std::string> katakanaAlphabet = { "ア", "イ", "ウ", "エ", "オ", "カ", "キ", "ク", "ケ", "コ", "サ", "シ", "ス", "セ", "ソ", "タ", "チ", "ツ", "テ", "ト", "ナ", "ニ", "ヌ", "ネ", "ノ", "ハ", "ヒ", "フ", "ヘ", "ホ", "マ", "ミ", "ム", "メ", "モ", "ヤ", "ユ", "ヨ", "ラ", "リ", "ル", "レ", "ロ", "ワ", "ヲ", "ン" };
//
//std::string generatePassword(int length, bool includeUppercase, std::vector<std::string>& alphabet) {
//    // Generador de números aleatorios
//    std::random_device rd;
//    std::mt19937 gen(rd());
//
//    // Generar la contraseña
//    std::string password;
//    for (int i = 0; i < length; ++i) {
//        password += alphabet[std::uniform_int_distribution<>(0, alphabet.size() - 1)(gen)];
//    }
//
//    return password;
//}
//
//int main() {
//    int length = 10;  // Longitud de la contraseña
//    bool includeUppercase = true;  // Incluir letras mayúsculas
//    int alphabetChoice = 0;  // 0 para latino, 1 para hiragana, 2 para katakana
//
//    std::vector<std::string> chosenAlphabet;
//    if (alphabetChoice == 0) {
//        chosenAlphabet = latinAlphabet;
//        if (includeUppercase) {
//            for (char c = 'A'; c <= 'Z'; ++c) {
//                chosenAlphabet.push_back(std::string(1, c));
//            }
//        }
//    }
//    else if (alphabetChoice == 1) {
//        chosenAlphabet = hiraganaAlphabet;
//    }
//    else if (alphabetChoice == 2) {
//        chosenAlphabet = katakanaAlphabet;
//    }
//
//    std::string password = generatePassword(length, includeUppercase, chosenAlphabet);
//
//    std::cout << "Contraseña generada: " << password << std::endl;
//
//    return 0;
//}
