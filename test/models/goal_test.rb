# == Schema Information
#
# Table name: goals
#
#  id            :bigint           not null, primary key
#  completed     :boolean          default(FALSE), not null
#  goal_amt      :integer          not null
#  goal_category :string           not null
#  notes         :string
#  target_date   :datetime
#  title         :string           not null
#  created_at    :datetime         not null
#  updated_at    :datetime         not null
#  account_id    :integer          not null
#
# Indexes
#
#  index_goals_on_account_id  (account_id)
#
require 'test_helper'

class GoalTest < ActiveSupport::TestCase
  # test "the truth" do
  #   assert true
  # end
end
