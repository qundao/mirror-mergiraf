classdef BasicClass
    properties
        Value
    end
    methods
<<<<<<< LEFT
        function r = fLeft(obj)
            r = cos([obj.Value]);
        end
||||||| BASE
=======
        function r = fRight(obj)
            r = sin([obj.Value]);
        end
>>>>>>> RIGHT
        function r = f1(obj,n)
            r = 2*n;
        end
    end
end