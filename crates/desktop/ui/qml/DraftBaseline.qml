pragma ComponentBehavior: Bound
import QtQml

QtObject {
    id: root
    property var value: null

    function capture(draft) { value = snapshot(draft); }
    function matches(draft) { return value !== null && equal(value, draft); }
    function clear() { value = null; }

    // Own the object structure without serializing another plaintext copy.
    // String values remain Qt/JS managed and cannot be reliably zeroized here.
    function snapshot(value) {
        if (value === null || typeof value !== "object")
            return value;
        if (Array.isArray(value))
            return value.map(item => root.snapshot(item));
        const result = {};
        for (const key of Object.keys(value))
            result[key] = root.snapshot(value[key]);
        return result;
    }

    function equal(left, right) {
        if (left === right)
            return true;
        if (left === null || right === null || typeof left !== "object" || typeof right !== "object")
            return false;
        if (Array.isArray(left) !== Array.isArray(right))
            return false;
        const leftKeys = Object.keys(left);
        const rightKeys = Object.keys(right);
        if (leftKeys.length !== rightKeys.length)
            return false;
        for (const key of leftKeys) {
            if (!Object.prototype.hasOwnProperty.call(right, key) || !root.equal(left[key], right[key]))
                return false;
        }
        return true;
    }

}
