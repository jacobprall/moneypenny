import { RECEIVE_GOALS, RECEIVE_GOAL, REMOVE_GOAL } from '../actions/goal_actions'

const goalsReducer = (oldState = {}, action) => {
  let newState = Object.assign({}, oldState);

  switch (action.type) {
    case RECEIVE_GOALS:
      newState = action.goals;
      return newState;
    case RECEIVE_GOAL:
      let newGoal = {[action.goal.id]: action.goal};
      newState = object.assign(newState, newGoal);
      return newState;
    case REMOVE_GOAL:
      delete newState[action.goalId];
      return newState;
    default:
      return newState;
  }
}


export default goalsReducer