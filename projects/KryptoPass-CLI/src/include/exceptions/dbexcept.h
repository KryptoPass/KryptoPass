#pragma once

#ifndef DB_EXCEPTION_H
#define DB_EXCEPTION_H

#include <stdexcept>
#include <string>

class DBException : public std::runtime_error {
public:
    explicit DBException(const std::string& msg)
        : std::runtime_error(msg) {}
};

#endif // DB_EXCEPTION_H
