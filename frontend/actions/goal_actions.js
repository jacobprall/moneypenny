import * as GoalAPIUtil from '../util/goal_api_util'

export const RECEIVE_GOALS = 'RECEIVE_GOALS';
export const RECEIVE_GOAL = 'RECEIVE_GOAL';
export const REMOVE_GOAL = 'REMOVE_GOALS';
export const RECEIVE_GOAL_ERRORS = 'RECEIVE_GOAL_ERRORS';
export const CLEAR_GOAL_ERRORS = 'CLEAR_GOAL_ERRORS';

export const receiveGoals = (goals) => ({
  type: RECEIVE_GOALS,
  goals
});

export const postGoal = (goal) => ({
  type: RECEIVE_GOAL,
  goal
});

export const patchGoal = goal => ({
  type: RECEIVE_GOAL,
  goal
});

export const removeGoal = goalId => ({
  type: REMOVE_GOAL,
  goalId
});

export const receiveGoalErrors = errors => ({
  type: RECEIVE_GOAL_ERRORS,
  errors
});

export const clearGoalErrors = () => ({
  type: CLEAR_GOAL_ERRORS
});


export const requestGoals = () => dispatch => (
  GoalAPIUtil.fetchGoals().then((goals) => dispatch(receiveGoals(goals)))
);

export const createGoal = goal => dispatch => (
  GoalAPIUtil.createGoal(goal).then(
    goal => dispatch(postGoal(goal)),
    errors => dispatch(receiveGoalErrors(errors.responseJSON))
  )
);

export const updateGoal = goal => dispatch => (
  GoalAPIUtil.updateGoal(goal).then(
    goal => dispatch(patchGoal(goal)),
    errors => dispatch(receiveGoalErrors(errors.responseJSON))
  )
);

export const deleteGoal = goalId => dispatch => (
  GoalAPIUtil.deleteGoal(goalId).then((goalId) => dispatch(removeGoal(goalId)))
);

