# == Schema Information
#
# Table name: bills
#
#  id         :bigint           not null, primary key
#  amount     :float            not null
#  due_date   :date             not null
#  name       :string           not null
#  recurring  :boolean          not null
#  created_at :datetime         not null
#  updated_at :datetime         not null
#  user_id    :integer          not null
#
# Indexes
#
#  index_bills_on_user_id  (user_id)
#
require 'test_helper'

class BillTest < ActiveSupport::TestCase
  # test "the truth" do
  #   assert true
  # end
end
