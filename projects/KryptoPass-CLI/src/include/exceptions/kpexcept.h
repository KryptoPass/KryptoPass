#pragma once

#ifndef KP_EXCEPTION_H
#define KP_EXCEPTION_H

#include <stdexcept>
#include <string>

class KPException : public std::runtime_error {
public:
    explicit KPException(const std::string& msg)
        : std::runtime_error(msg) {}
};

#endif // KP_EXCEPTION_H
