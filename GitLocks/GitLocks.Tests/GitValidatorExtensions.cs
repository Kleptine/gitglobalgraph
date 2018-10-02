using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Conditions;
using Optional;

namespace GitLocks.Tests
{
    public static class GitValidatorExtensions
    {
        public static ConditionValidator<Option<T>> HasValue<T>(this ConditionValidator<Option<T>> validator)
        {
            if (!validator.Value.HasValue)
                OptionalShouldHaveValue(validator, $"There should be a value present in the optional. Found [None].");
            return validator;
        }

        public static ConditionValidator<Option<T, TException>> HasValue<T, TException>(
            this ConditionValidator<Option<T, TException>> validator) where TException : Exception
        {
            validator.Value.MatchNone(exception =>
            {
                OptionalShouldHaveValue(validator,
                    $"There should be a value present in the optional. Found [{typeof(TException)}]. Optional exception:\n" +
                    $" Message: {exception.Message}\n" +
                    $" Stack trace: \n {exception.StackTrace}\n");
            });
            return validator;
        }

        public static ConditionValidator<Option<T, TException>> HasExceptionWithReason<T, TException>(
            this ConditionValidator<Option<T, TException>> validator, GitConflictException.ExceptionReason reason)
            where TException : GitConflictException
        {
            validator.Value.Match(
                _ =>
                {
                    validator.ThrowException(
                        $"Optional object contained a value, but we expected the object to contain an exception with ExceptionReason: [{reason}].");
                },
                exception =>
                {
                    if (exception.Reason != reason)
                    {
                        validator.ThrowException(
                            $"Optional object contained exception, but with an incorrect ExceptionReason. Found [{exception.Reason}]. Expected [{reason}]. " +
                            $"Optional exception stack trace: \n {exception.StackTrace}");
                    }
                });
            return validator;
        }

        private static void OptionalShouldHaveValue<T>(ConditionValidator<T> validator, string conditionDescription)
        {
            string conditionMessage = $"[{nameof(OptionalShouldHaveValue)}]: {conditionDescription}";
            validator.ThrowException(conditionMessage);
        }
    }
}
