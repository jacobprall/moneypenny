
export const fetchGoals = () => (
  $.ajax({
    url: '/api/goals'
  })
);

export const createGoal = goal => (
  $.ajax({
    url: 'api/goals',
    method: 'POST',
    data: {
      goal
    }
  })
);

export const updateGoal = goal => (
  $.ajax({
    url: `api/goals/${goal.id}`,
    method: 'PATCH',
    data: {
      goal
    }
  })
);

export const deleteGoal = goalId => (
  $.ajax({
    url: `api/goals/${goalId}`,
    method: 'DELETE'
  })
);