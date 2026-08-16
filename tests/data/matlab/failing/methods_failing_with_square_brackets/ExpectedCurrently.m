classdef BasicClass
    methods
<<<<<<< LEFT
        function r = fLeft(obj)
            r = round([obj.Value],2);
        end
||||||| BASE
=======
        function r = fRight(obj)
            r = ceil([obj.Value],2);
        end
>>>>>>> RIGHT
        function r = f_a(obj,n)
            r = 100*n;
        end
    end
end